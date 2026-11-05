//! Send an audio file to OpenRouter and print the transcript, before and
//! after the cleanup step.
//! cargo run --example transcribe -- <file> [model] [language]
//! Reads OPENROUTER_API_KEY (and the other MY_WHISPER_* settings) from the
//! environment or `.env`.

use my_whisper_lib::config::{self, Config};
use my_whisper_lib::pipeline;
use my_whisper_lib::stt::{OpenRouter, SpeechToText};

fn main() {
    config::load_dotenv();
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: transcribe <file> [model] [language]");
    let mut cfg = Config::load();
    if let Some(m) = args.get(2) {
        cfg.model = m.clone();
    }
    if let Some(l) = args.get(3) {
        cfg.language = l.clone();
    }

    let audio = std::fs::read(path).expect("read audio file");
    let format = path.rsplit('.').next().unwrap_or("wav").to_lowercase();
    let lang = Some(cfg.language.clone()).filter(|l| !l.is_empty());
    let stt = OpenRouter::new(cfg.api_key.clone(), cfg.model.clone(), lang).with_prompt(cfg.stt_prompt());

    let started = std::time::Instant::now();
    let t = match stt.transcribe(&audio, &format) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    println!("stt: {}  {:.1}s  cost {:?}", cfg.model, started.elapsed().as_secs_f32(), t.cost);
    println!("raw:   {}", t.text);

    if cfg.cleanup {
        let started = std::time::Instant::now();
        let cleaned = pipeline::process(&t.text, &cfg);
        println!("cleanup: {}  {:.1}s", cfg.cleanup_model, started.elapsed().as_secs_f32());
        println!("clean: {cleaned}");
    }
}
