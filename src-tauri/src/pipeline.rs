//! What happens to the text between transcription and paste.
//!
//! 1. Tidy whitespace.
//! 2. Optional cleanup by a fast LLM: English terms that the speech model
//!    wrote in Persian letters go back to Latin script (کامیت → commit).
//!
//! Translation or other rewrites plug in here later as extra steps.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::config::Config;

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(15);

/// Tested on Persian dictation: fixed commit/record/Space/API/OpenRouter and
/// misheard "کیک" → "گیگ" without changing other words.
const CLEANUP_SYSTEM: &str = "You fix speech-to-text output. The text is Persian dictation. \
Rewrite English words and technical terms that were transliterated into Persian letters \
back into Latin script with correct English spelling (e.g. کامیت → commit, اسپیس → Space, \
ای پی آی → API, اوپن روتر → OpenRouter). Keep everything else exactly as spoken: \
do not translate, summarize, rephrase, or answer. Fix obvious misheard words only when the \
intended word is clear from context. Output only the corrected text.";

pub fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Runs every enabled step. A failed cleanup falls back to the raw text, so a
/// network hiccup never loses what the user said.
pub fn process(text: &str, cfg: &Config) -> String {
    let text = tidy(text);
    if !cfg.cleanup || text.is_empty() {
        return text;
    }
    let started = std::time::Instant::now();
    match cleanup(&text, cfg) {
        Ok(cleaned) if plausible(&text, &cleaned) => {
            eprintln!("my-whisper: cleanup in {:.1}s", started.elapsed().as_secs_f32());
            cleaned
        }
        Ok(cleaned) => {
            eprintln!("my-whisper: cleanup result rejected, using raw text: {cleaned}");
            text
        }
        Err(e) => {
            eprintln!("my-whisper: cleanup failed, using raw text: {e}");
            text
        }
    }
}

/// Guards against the model answering or summarizing instead of correcting.
fn plausible(input: &str, output: &str) -> bool {
    let (a, b) = (input.chars().count() as f32, output.chars().count() as f32);
    !output.is_empty() && b >= a * 0.6 && b <= a * 1.5
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

fn cleanup(text: &str, cfg: &Config) -> Result<String, String> {
    let mut system = CLEANUP_SYSTEM.to_string();
    let vocab = cfg.vocabulary_words();
    if !vocab.is_empty() {
        system.push_str(&format!(" Spell these exactly when they occur: {}.", vocab.join(", ")));
    }

    let body = json!({
        "model": cfg.cleanup_model,
        "temperature": 0,
        "max_tokens": 200 + text.len(),
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": text },
        ],
    });

    let resp = reqwest::blocking::Client::builder()
        .timeout(CLEANUP_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .post("https://openrouter.ai/api/v1/chat/completions")
        .bearer_auth(cfg.api_key.trim())
        .header("X-Title", "my-whisper")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let raw = resp.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{status}: {}", raw.chars().take(200).collect::<String>()));
    }
    let parsed: ChatResponse = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or("empty response")?;
    Ok(tidy(&content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace() {
        assert_eq!(tidy("  سلام   دنیا \n"), "سلام دنیا");
    }

    #[test]
    fn rejects_answers_and_summaries() {
        let input = "بعدش کامیت و اینا انجام بده و فایل‌هایی که رکورد می‌کنی رو پاک کن";
        assert!(plausible(input, "بعدش commit و اینا انجام بده و فایل‌هایی که record می‌کنی رو پاک کن"));
        assert!(!plausible(input, "باشه"));
        assert!(!plausible(input, &input.repeat(3)));
        assert!(!plausible(input, ""));
    }
}
