//! Play the start, stop and error chimes: cargo run --example chime
use my_whisper_lib::sound::{play_blocking, Chime};

fn main() {
    for (name, c) in [("start", Chime::Start), ("stop", Chime::Stop), ("error", Chime::Error)] {
        println!("{name}: {:?}", play_blocking(c));
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}
