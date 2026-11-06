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

/// v2, tested on five real dictations with gemini-3.1-flash-lite: converted
/// CPU, RAM, bottleneck, Windows, record, commit, deploy, Docker in place and
/// left plain Persian alone. v1 missed some terms and added "(bottleneck)"
/// in parentheses, hence the explicit rules and examples.
const CLEANUP_SYSTEM: &str = "You post-process Persian speech-to-text output.

Task: find English words and technical terms that the speech model wrote with Persian letters, \
and replace each one in place with the English word in Latin letters.

Rules:
- Replace in place. Never keep the Persian spelling next to the English one, never add parentheses, notes, or explanations.
- Convert every such term, including common ones: brand and product names (Windows, Docker), \
computer terms (CPU, RAM, bottleneck, deploy, commit, record, switch), keys (Alt, Space, Ctrl).
- Keep very common Persian loanwords that are normal in written Persian (فایل، تیم، سرور، ایمیل).
- Keep Persian grammar attached to the word («رکورد می‌کنی» → «record می‌کنی»).
- Fix a misheard word only when the intended word is obvious from context.
- Do not translate, summarize, rephrase, reorder, or answer the text. If nothing needs changing, return it unchanged.
- Output only the final text.

Examples:
Input: دکمه‌های آلت و اسپیس و ویندوز رو بزن تا رکورد بشه.
Output: دکمه‌های Alt و Space و Windows رو بزن تا record بشه.

Input: سی پی یو و رم باید طوری باشن که باتلنک نداشته باشیم.
Output: CPU و RAM باید طوری باشن که bottleneck نداشته باشیم.

Input: سلام، فردا جلسه داریم؟
Output: سلام، فردا جلسه داریم؟";

pub fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Runs every enabled step. A failed cleanup falls back to the raw text, so a
/// network hiccup never loses what the user said.
pub fn process(text: &str, cfg: &Config) -> String {
    process_at(crate::stt::OpenRouter::DEFAULT_BASE_URL, text, cfg)
}

fn process_at(base_url: &str, text: &str, cfg: &Config) -> String {
    let text = tidy(text);
    if !cfg.cleanup || text.is_empty() {
        return text;
    }
    let started = std::time::Instant::now();
    match cleanup(base_url, &text, cfg) {
        Ok(cleaned) if plausible(&text, &cleaned) => {
            crate::log::info(&format!("cleanup in {:.1}s", started.elapsed().as_secs_f32()));
            cleaned
        }
        Ok(cleaned) => {
            crate::log::warn("cleanup result rejected, using raw text");
            crate::log::private(&format!("rejected cleanup: {cleaned}"));
            text
        }
        Err(e) => {
            crate::log::warn(&format!("cleanup failed, using raw text: {e}"));
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

fn cleanup(base_url: &str, text: &str, cfg: &Config) -> Result<String, String> {
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
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(cfg.api_key.trim())
        .header("X-Title", "Zemzeme")
        .json(&body)
        .send()
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let raw = resp.text().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("{status}: {}", raw.chars().take(200).collect::<String>()));
    }
    let parsed: ChatResponse = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let content = parsed.choices.into_iter().next().and_then(|c| c.message.content).ok_or("empty response")?;
    Ok(tidy(&content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::serve;

    const RAW: &str = "بعدش کامیت و اینا انجام بده";

    fn cfg() -> Config {
        Config { api_key: "sk-test".into(), vocabulary: "Zemzeme".into(), ..Config::default() }
    }

    fn reply(content: &str) -> String {
        serde_json::json!({ "choices": [{ "message": { "content": content } }] }).to_string()
    }

    #[test]
    fn uses_cleaned_text() {
        let (url, server) = serve(200, &reply("بعدش commit و اینا انجام بده"), Duration::ZERO);
        assert_eq!(process_at(&url, RAW, &cfg()), "بعدش commit و اینا انجام بده");

        let req = server.join().unwrap();
        assert!(req.head.starts_with("POST /chat/completions"));
        assert_eq!(req.body["model"], crate::config::DEFAULT_CLEANUP_MODEL);
        assert_eq!(req.body["messages"][1]["content"], RAW);
        assert!(req.body["messages"][0]["content"].as_str().unwrap().contains("Zemzeme"), "vocabulary is sent");
    }

    #[test]
    fn keeps_raw_text_when_cleanup_fails_or_misbehaves() {
        for (status, body) in [(500, "{}".to_string()), (200, reply("باشه")), (200, "not json".to_string())] {
            let (url, server) = serve(status, &body, Duration::ZERO);
            assert_eq!(process_at(&url, RAW, &cfg()), RAW, "status {status}, body {body}");
            server.join().unwrap();
        }
    }

    #[test]
    fn skips_cleanup_when_off() {
        let off = Config { cleanup: false, ..cfg() };
        // Nothing listens on this address; a request would fail the test by hanging or erroring.
        assert_eq!(process_at("http://127.0.0.1:1", "  سلام   دنیا ", &off), "سلام دنیا");
    }

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
