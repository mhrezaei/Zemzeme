//! App settings.
//!
//! For now everything comes from environment variables (and `.env` in debug
//! builds). Phase 6 replaces this with the settings window and the OS keyring.

use crate::inject::PasteKey;

pub const DEFAULT_MODEL: &str = "openai/gpt-4o-transcribe";
pub const DEFAULT_LANGUAGE: &str = "fa";
/// Fastest and most faithful in our test (1.3–1.7 s, ~$0.0001 per message).
pub const DEFAULT_CLEANUP_MODEL: &str = "google/gemini-3.1-flash-lite";

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub model: String,
    /// Empty means auto-detect.
    pub language: String,
    pub microphone: Option<String>,
    pub paste_key: PasteKey,
    /// Send the transcript through a fast LLM that writes English terms in
    /// Latin letters (کامیت → commit). See `pipeline.rs`.
    pub cleanup: bool,
    pub cleanup_model: String,
    /// Comma-separated words to spell exactly (names, jargon).
    pub vocabulary: String,
}

fn parse_bool(v: &str) -> bool {
    !matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "off" | "false" | "no")
}

impl Config {
    pub fn load() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Self {
            api_key: var("OPENROUTER_API_KEY").unwrap_or_default(),
            model: var("MY_WHISPER_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into()),
            language: std::env::var("MY_WHISPER_LANG").unwrap_or_else(|_| DEFAULT_LANGUAGE.into()),
            microphone: var("MY_WHISPER_MIC"),
            paste_key: PasteKey::parse(&var("MY_WHISPER_PASTE_KEY").unwrap_or_default()),
            cleanup: var("MY_WHISPER_CLEANUP").map(|v| parse_bool(&v)).unwrap_or(true),
            cleanup_model: var("MY_WHISPER_CLEANUP_MODEL")
                .unwrap_or_else(|| DEFAULT_CLEANUP_MODEL.into()),
            vocabulary: var("MY_WHISPER_VOCAB").unwrap_or_default(),
        }
    }

    pub fn vocabulary_words(&self) -> Vec<&str> {
        self.vocabulary.split([',', '،']).map(str::trim).filter(|w| !w.is_empty()).collect()
    }

    /// Spelling hint sent to the speech model. A general "write English in
    /// Latin letters" instruction did not work reliably there (the cleanup
    /// step does that), so only the user's vocabulary is sent.
    pub fn stt_prompt(&self) -> Option<String> {
        let words = self.vocabulary_words();
        (!words.is_empty()).then(|| format!("واژه‌های پرکاربرد: {}.", words.join("، ")))
    }
}

/// In debug builds, read `.env` from the working directory or any parent.
pub fn load_dotenv() {
    if cfg!(debug_assertions) {
        let _ = dotenvy::dotenv();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(vocabulary: &str) -> Config {
        Config {
            api_key: String::new(),
            model: DEFAULT_MODEL.into(),
            language: DEFAULT_LANGUAGE.into(),
            microphone: None,
            paste_key: PasteKey::CtrlV,
            cleanup: true,
            cleanup_model: DEFAULT_CLEANUP_MODEL.into(),
            vocabulary: vocabulary.into(),
        }
    }

    #[test]
    fn empty_vocabulary_sends_no_prompt() {
        assert_eq!(cfg(" , ").stt_prompt(), None);
    }

    #[test]
    fn vocabulary_splits_on_both_commas() {
        assert_eq!(cfg("Tauri, my-whisper،هادی").vocabulary_words(), ["Tauri", "my-whisper", "هادی"]);
    }

    #[test]
    fn parses_switch_values() {
        assert!(!parse_bool("off") && !parse_bool("0") && !parse_bool("False"));
        assert!(parse_bool("on") && parse_bool("1"));
    }
}
