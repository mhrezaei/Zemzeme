//! Play the start, stop and error chimes: cargo run --example chime
use zemzeme_lib::sound::{Chime, play_blocking};

fn main() {
    for (name, c) in [("start", Chime::Start), ("stop", Chime::Stop), ("error", Chime::Error)] {
        println!("{name}: {:?}", play_blocking(c));
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}
