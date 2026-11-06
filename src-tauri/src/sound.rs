//! Short start/stop chimes, synthesized so no audio files are needed.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

#[derive(Clone, Copy)]
pub enum Chime {
    /// Rising two notes.
    Start,
    /// Falling two notes.
    Stop,
    /// One low note.
    Error,
}

const VOLUME: f32 = 0.12;
const NOTE_SECS: f32 = 0.075;

fn notes(chime: Chime) -> &'static [f32] {
    match chime {
        Chime::Start => &[880.0, 1318.5],
        Chime::Stop => &[1318.5, 880.0],
        Chime::Error => &[330.0],
    }
}

/// Mono samples at `rate`: sine notes with a short fade in/out so they don't click.
fn synth(chime: Chime, rate: u32) -> Vec<f32> {
    let per_note = (rate as f32 * NOTE_SECS) as usize;
    let fade = per_note / 6;
    let mut out = Vec::with_capacity(per_note * 2);
    for &freq in notes(chime) {
        for i in 0..per_note {
            let env = if i < fade {
                i as f32 / fade as f32
            } else if i > per_note - fade {
                (per_note - i) as f32 / fade as f32
            } else {
                1.0
            };
            let t = i as f32 / rate as f32;
            out.push((t * freq * std::f32::consts::TAU).sin() * env * VOLUME);
        }
    }
    out
}

/// Plays in the background; failures are only logged.
pub fn play(chime: Chime) {
    std::thread::spawn(move || {
        if let Err(e) = play_blocking(chime) {
            crate::log::warn(&format!("sound: {e}"));
        }
    });
}

pub fn play_blocking(chime: Chime) -> Result<(), String> {
    let device = cpal::default_host().default_output_device().ok_or("no output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let samples = synth(chime, config.sample_rate);
    let duration = Duration::from_secs_f32(samples.len() as f32 / config.sample_rate as f32);

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, samples),
        SampleFormat::I16 => build::<i16>(&device, &config, samples),
        SampleFormat::I32 => build::<i32>(&device, &config, samples),
        SampleFormat::U16 => build::<u16>(&device, &config, samples),
        other => return Err(format!("unsupported output format {other}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    // Let the buffer drain before the stream is dropped.
    std::thread::sleep(duration + Duration::from_millis(120));
    Ok(())
}

fn build<T>(device: &cpal::Device, config: &cpal::StreamConfig, samples: Vec<f32>) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let pos = Arc::new(Mutex::new(0usize));
    device
        .build_output_stream::<T, _, _>(
            *config,
            move |data: &mut [T], _| {
                let mut p = pos.lock().unwrap();
                for frame in data.chunks_mut(channels) {
                    let v = samples.get(*p).copied().unwrap_or(0.0);
                    *p += 1;
                    for s in frame {
                        *s = T::from_sample(v);
                    }
                }
            },
            |e| crate::log::warn(&format!("sound stream: {e}")),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chimes_are_short_quiet_and_click_free() {
        for c in [Chime::Start, Chime::Stop, Chime::Error] {
            let s = synth(c, 48_000);
            assert!(s.len() <= 48_000 / 5, "under 0.2 s");
            assert!(s.iter().all(|v| v.abs() <= VOLUME + 1e-6));
            assert!(s[0].abs() < 1e-3 && s.last().unwrap().abs() < 0.01, "fades in and out");
        }
    }
}
