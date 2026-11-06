//! Record from a microphone for a few seconds and report what was captured.
//! cargo run --example record -- [seconds] [device-id] [out.wav]

use std::time::Duration;

use zemzeme_lib::audio::Recorder;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let secs: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let device = args.get(2).map(String::as_str).filter(|s| *s != "default");
    let out = args.get(3);

    let rec = Recorder::start(device, |_| {}).expect("start");
    std::thread::sleep(Duration::from_secs_f32(secs));
    let rec = rec.stop().expect("stop");

    let peak = rec.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    let rms = (rec.samples.iter().map(|s| s * s).sum::<f32>() / rec.samples.len().max(1) as f32).sqrt();
    println!(
        "asked {secs}s, got {:.2}s, peak {peak:.4}, rms {rms:.5}, speech {:.2}s",
        rec.duration_secs(),
        rec.speech_seconds()
    );

    if let Some(path) = out {
        std::fs::write(path, rec.to_wav()).expect("write wav");
        println!("saved {path}");
    }
}
