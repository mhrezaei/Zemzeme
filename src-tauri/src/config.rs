//! App settings.
//!
//! For now everything comes from environment variables (and `.env` in debug
//! builds). Phase 6 replaces this with the settings window and the OS keyring.

pub const DEFAULT_MODEL: &str = "openai/gpt-4o-transcribe";
pub const DEFAULT_LANGUAGE: &str = "fa";

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub model: String,
    /// Empty means auto-detect.
    pub language: String,
    pub microphone: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Self {
            api_key: var("OPENROUTER_API_KEY").unwrap_or_default(),
            model: var("MY_WHISPER_MODEL").unwrap_or_else(|| DEFAULT_MODEL.into()),
            language: std::env::var("MY_WHISPER_LANG").unwrap_or_else(|_| DEFAULT_LANGUAGE.into()),
            microphone: var("MY_WHISPER_MIC"),
        }
    }
}

/// In debug builds, read `.env` from the working directory or any parent.
pub fn load_dotenv() {
    if cfg!(debug_assertions) {
        let _ = dotenvy::dotenv();
    }
}
