//! Send an audio file to OpenRouter and print the transcript.
//! cargo run --example transcribe -- <file> [model] [language]
//! Reads OPENROUTER_API_KEY from the environment or `.env`.

use my_whisper_lib::config::{self, Config};
use my_whisper_lib::stt::{OpenRouter, SpeechToText};

fn main() {
    config::load_dotenv();
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: transcribe <file> [model] [language]");
    let cfg = Config::load();
    let model = args.get(2).cloned().unwrap_or(cfg.model);
    let lang = args.get(3).cloned().unwrap_or(cfg.language);

    let audio = std::fs::read(path).expect("read audio file");
    let format = path.rsplit('.').next().unwrap_or("wav").to_lowercase();
    let stt = OpenRouter::new(cfg.api_key, model.clone(), Some(lang).filter(|l| !l.is_empty()));

    let started = std::time::Instant::now();
    match stt.transcribe(&audio, &format) {
        Ok(t) => {
            println!("model: {model}  time: {:.1}s  cost: {:?}", started.elapsed().as_secs_f32(), t.cost);
            println!("{}", t.text);
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
