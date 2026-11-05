//! Microphone capture.
//!
//! A cpal stream can't move between threads, so each recording runs on its own
//! thread that owns the stream. Samples are mixed to mono f32 in the audio
//! callback; on stop they are resampled to 16 kHz and encoded as 16-bit WAV.

use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use serde::Serialize;

/// Speech models are trained on 16 kHz audio; higher rates only add upload size.
pub const TARGET_RATE: u32 = 16_000;

#[derive(Debug, Clone, Serialize)]
pub struct Microphone {
    /// Stable id to store in settings.
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Prefix for microphones addressed by PipeWire node name.
const PW_PREFIX: &str = "pw:";

pub fn list_microphones() -> Vec<Microphone> {
    #[cfg(target_os = "linux")]
    if let Some(mics) = pipewire_sources() {
        return mics;
    }
    list_cpal_microphones()
}

/// ALSA's own device list is long and full of duplicates (hw:, plughw:,
/// dsnoop:…). PipeWire knows the real microphones and their proper names,
/// so ask it via `pw-dump`. The node name is stable across reboots and
/// replugging, unlike the numeric id.
#[cfg(target_os = "linux")]
fn pipewire_sources() -> Option<Vec<Microphone>> {
    let out = std::process::Command::new("pw-dump").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let objects: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).ok()?;

    let default_name = objects
        .iter()
        .filter(|o| o["type"] == "PipeWire:Interface:Metadata")
        .flat_map(|o| o["metadata"].as_array().cloned().unwrap_or_default())
        .find(|m| m["key"] == "default.audio.source")
        .and_then(|m| m["value"]["name"].as_str().map(String::from));

    let mics: Vec<Microphone> = objects
        .iter()
        .filter_map(|o| {
            let props = &o["info"]["props"];
            if props["media.class"] != "Audio/Source" {
                return None;
            }
            let node = props["node.name"].as_str()?;
            let name = props["node.description"].as_str().or(props["node.nick"].as_str()).unwrap_or(node);
            Some(Microphone {
                id: format!("{PW_PREFIX}{node}"),
                name: name.to_string(),
                is_default: default_name.as_deref() == Some(node),
            })
        })
        .collect();
    Some(mics)
}

fn list_cpal_microphones() -> Vec<Microphone> {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|d| d.id().ok())
        .map(|id| id.to_string());

    let Ok(devices) = host.input_devices() else { return Vec::new() };
    devices
        .filter_map(|d| {
            let id = d.id().ok()?.to_string();
            let name = d.description().map(|x| x.name().to_string()).unwrap_or_else(|_| id.clone());
            Some(Microphone { is_default: Some(&id) == default_id.as_ref(), id, name })
        })
        .collect()
}

fn find_device(id: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    let id = id.filter(|s| !s.is_empty());

    // A PipeWire microphone: open ALSA's default device (which is PipeWire)
    // and tell PipeWire's ALSA plugin which node to connect to. The plugin
    // reads PIPEWIRE_NODE when the stream opens.
    #[cfg(target_os = "linux")]
    {
        // SAFETY: only this function writes the variable, and nothing else
        // in the app reads it; the audio library reads it while opening.
        match id.and_then(|i| i.strip_prefix(PW_PREFIX)) {
            Some(node) => unsafe { std::env::set_var("PIPEWIRE_NODE", node) },
            None => unsafe { std::env::remove_var("PIPEWIRE_NODE") },
        }
    }
    let id = id.filter(|i| !i.starts_with(PW_PREFIX));

    if let Some(id) = id {
        if let Some(dev) = id.parse().ok().and_then(|id| host.device_by_id(&id)) {
            return Ok(dev);
        }
        crate::log::warn(&format!("microphone `{id}` not found, using default"));
    }
    host.default_input_device().ok_or_else(|| "No microphone found".to_string())
}

/// Finished recording, mono f32 at `TARGET_RATE`.
pub struct Recording {
    pub samples: Vec<f32>,
}

impl Recording {
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / TARGET_RATE as f32
    }

    /// True when every sample is (near) zero: the device delivered no signal.
    pub fn is_silent(&self) -> bool {
        self.samples.iter().all(|s| s.abs() < 1e-5)
    }

    /// Rough amount of speech, in seconds: 30 ms frames clearly louder than the
    /// background. Speech models invent text from pure room noise, so clips
    /// without speech should not be sent at all.
    ///
    /// The background level is the 20th-percentile frame, capped so a clip
    /// that is speech from start to end still counts. Measured on a USB mic:
    /// room noise gave 0 loud frames, a 16 s sentence gave 5.8 s.
    pub fn speech_seconds(&self) -> f32 {
        const FRAME: usize = (TARGET_RATE as usize * 30) / 1000;
        const LOUDER_THAN_FLOOR: f32 = 2.5;

        let rms: Vec<f32> = self
            .samples
            .chunks_exact(FRAME)
            .map(|f| (f.iter().map(|s| s * s).sum::<f32>() / FRAME as f32).sqrt())
            .collect();
        if rms.is_empty() {
            return 0.0;
        }
        let mut sorted = rms.clone();
        sorted.sort_by(f32::total_cmp);
        let floor = sorted[sorted.len() / 5].clamp(0.001, 0.006);
        let loud = rms.iter().filter(|&&r| r > floor * LOUDER_THAN_FLOOR).count();
        loud as f32 * 0.03
    }

    pub fn to_wav(&self) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: TARGET_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buf = Cursor::new(Vec::with_capacity(self.samples.len() * 2 + 44));
        {
            let mut w = hound::WavWriter::new(&mut buf, spec).expect("wav header");
            for &s in &self.samples {
                w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).expect("wav sample");
            }
            w.finalize().expect("wav finalize");
        }
        buf.into_inner()
    }
}

/// A recording in progress. Drop or call `stop` to end it.
pub struct Recorder {
    stop_tx: mpsc::Sender<()>,
    handle: Option<JoinHandle<Result<Recording, String>>>,
}

impl Recorder {
    /// Starts capturing. `on_level` is called about 20 times a second with a 0..1 loudness value.
    pub fn start(
        device_id: Option<&str>,
        on_level: impl Fn(f32) + Send + 'static,
    ) -> Result<Self, String> {
        let device = find_device(device_id)?;
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        // Report whether the stream opened before returning, so errors reach the caller.
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

        let handle = std::thread::Builder::new()
            .name("my-whisper-audio".into())
            .spawn(move || {
                let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(48_000 * 60)));
                let level = Arc::new(AtomicU32::new(0));

                let opened = open_stream(&device, buffer.clone(), level.clone());
                let (stream, rate) = match opened {
                    Ok(v) => {
                        let _ = ready_tx.send(Ok(()));
                        v
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.clone()));
                        return Err(e);
                    }
                };

                loop {
                    match stop_rx.recv_timeout(Duration::from_millis(50)) {
                        Err(RecvTimeoutError::Timeout) => {
                            on_level(f32::from_bits(level.load(Ordering::Relaxed)));
                        }
                        _ => break,
                    }
                }
                drop(stream);

                let raw = std::mem::take(&mut *buffer.lock().unwrap());
                Ok(Recording { samples: resample(&raw, rate, TARGET_RATE) })
            })
            .map_err(|e| e.to_string())?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { stop_tx, handle: Some(handle) }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("Audio thread exited unexpectedly".into()),
        }
    }

    pub fn stop(mut self) -> Result<Recording, String> {
        let _ = self.stop_tx.send(());
        self.handle
            .take()
            .expect("handle")
            .join()
            .map_err(|_| "Audio thread panicked".to_string())?
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
    }
}

fn open_stream(
    device: &cpal::Device,
    buffer: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<(cpal::Stream, u32), String> {
    let supported = device.default_input_config().map_err(|e| format!("Microphone: {e}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    let rate = config.sample_rate;

    let err_fn = |e: cpal::Error| crate::log::warn(&format!("audio stream error: {e}"));

    fn build<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        channels: usize,
        buffer: Arc<Mutex<Vec<f32>>>,
        level: Arc<AtomicU32>,
        err_fn: impl FnMut(cpal::Error) + Send + 'static,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        device.build_input_stream::<T, _, _>(
            config.clone(),
            move |data: &[T], _| {
                let mut sum_sq = 0.0f32;
                let mut buf = buffer.lock().unwrap();
                for frame in data.chunks(channels) {
                    let mono = frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32;
                    sum_sq += mono * mono;
                    buf.push(mono);
                }
                let frames = (data.len() / channels).max(1);
                let rms = (sum_sq / frames as f32).sqrt();
                // Map RMS to a 0..1 meter: speech sits around 0.02–0.2.
                let meter = (rms * 8.0).sqrt().min(1.0);
                level.store(meter.to_bits(), Ordering::Relaxed);
            },
            err_fn,
            None,
        )
    }

    let stream = match format {
        SampleFormat::F32 => build::<f32>(device, &config, channels, buffer, level, err_fn),
        SampleFormat::I16 => build::<i16>(device, &config, channels, buffer, level, err_fn),
        SampleFormat::I32 => build::<i32>(device, &config, channels, buffer, level, err_fn),
        SampleFormat::U16 => build::<u16>(device, &config, channels, buffer, level, err_fn),
        SampleFormat::I8 => build::<i8>(device, &config, channels, buffer, level, err_fn),
        SampleFormat::U8 => build::<u8>(device, &config, channels, buffer, level, err_fn),
        other => return Err(format!("Unsupported sample format {other}")),
    }
    .map_err(|e| format!("Microphone: {e}"))?;

    stream.play().map_err(|e| format!("Microphone: {e}"))?;
    Ok((stream, rate))
}

/// Downsample by averaging each output sample's input window (a box low-pass),
/// which is enough for speech and avoids pulling in a DSP crate.
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = (input.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let start = (i as f64 * ratio) as usize;
        let end = (((i + 1) as f64 * ratio) as usize).clamp(start + 1, input.len());
        let window = &input[start..end];
        out.push(window.iter().sum::<f32>() / window.len() as f32);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_48k_to_16k_keeps_duration() {
        let input = vec![0.5f32; 48_000];
        let out = resample(&input, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resample_44_1k_to_16k() {
        let out = resample(&vec![0.0f32; 44_100], 44_100, 16_000);
        assert_eq!(out.len(), 16_000);
    }

    /// Deterministic pseudo-noise in -amp..amp.
    fn noise(len: usize, amp: f32) -> Vec<f32> {
        let mut x: u32 = 12345;
        (0..len)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                ((x >> 16) as f32 / 32_768.0 - 1.0) * amp
            })
            .collect()
    }

    #[test]
    fn room_noise_has_no_speech() {
        let rec = Recording { samples: noise(16_000 * 3, 0.006) };
        assert!(rec.speech_seconds() < 0.1, "{}", rec.speech_seconds());
    }

    #[test]
    fn loud_section_counts_as_speech() {
        let mut samples = noise(16_000, 0.006);
        samples.extend(noise(16_000, 0.08)); // 1 s of "speech"
        samples.extend(noise(16_000, 0.006));
        let s = Recording { samples }.speech_seconds();
        assert!((0.8..1.1).contains(&s), "{s}");
    }

    #[test]
    fn continuous_speech_counts() {
        let s = Recording { samples: noise(16_000 * 2, 0.08) }.speech_seconds();
        assert!(s > 1.5, "{s}");
    }

    #[test]
    fn wav_has_header_and_samples() {
        let rec = Recording { samples: vec![0.0; 1600] };
        let wav = rec.to_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 1600 * 2);
    }
}
