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

pub fn list_microphones() -> Vec<Microphone> {
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
    if let Some(id) = id.filter(|s| !s.is_empty()) {
        if let Some(dev) = id.parse().ok().and_then(|id| host.device_by_id(&id)) {
            return Ok(dev);
        }
        eprintln!("my-whisper: microphone `{id}` not found, using default");
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

    let err_fn = |e: cpal::Error| eprintln!("my-whisper: audio stream error: {e}");

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

    #[test]
    fn wav_has_header_and_samples() {
        let rec = Recording { samples: vec![0.0; 1600] };
        let wav = rec.to_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 1600 * 2);
    }
}
