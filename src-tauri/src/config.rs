//! App settings.
//!
//! Settings live in `~/.config/dev.hadi.mywhisper/settings.json`. The API key
//! is kept in the OS keyring (GNOME Keyring / macOS Keychain / Windows
//! Credential Manager), never in the file.
//!
//! On first run, values from environment variables (and `.env` in debug
//! builds) seed the file, so earlier `.env` setups carry over.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::inject::PasteKey;

pub const DEFAULT_MODEL: &str = "openai/gpt-4o-transcribe";
pub const DEFAULT_LANGUAGE: &str = "fa";
/// Fastest and most faithful in our test (1.3–1.7 s, ~$0.0001 per message).
pub const DEFAULT_CLEANUP_MODEL: &str = "google/gemini-3.1-flash-lite";

const APP_ID: &str = "dev.hadi.mywhisper";
const KEYRING_SERVICE: &str = "my-whisper";
const KEYRING_USER: &str = "openrouter-api-key";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// From the keyring (or `OPENROUTER_API_KEY`); never written to the file.
    #[serde(skip)]
    pub api_key: String,
    pub model: String,
    /// Empty means auto-detect.
    pub language: String,
    /// Microphone id from `audio::list_microphones`; empty = system default.
    pub microphone: String,
    pub paste_key: PasteKey,
    /// Send the transcript through a fast LLM that writes English terms in
    /// Latin letters (کامیت → commit). See `pipeline.rs`.
    pub cleanup: bool,
    pub cleanup_model: String,
    /// Comma-separated words to spell exactly (names, jargon).
    pub vocabulary: String,
    /// Keep every recording on disk (for troubleshooting). Off by default.
    pub keep_audio: bool,
    /// Short chimes when recording starts and stops.
    pub sounds: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: DEFAULT_MODEL.into(),
            language: DEFAULT_LANGUAGE.into(),
            microphone: String::new(),
            paste_key: PasteKey::CtrlV,
            cleanup: true,
            cleanup_model: DEFAULT_CLEANUP_MODEL.into(),
            vocabulary: String::new(),
            keep_audio: false,
            sounds: true,
        }
    }
}

fn parse_bool(v: &str) -> bool {
    !matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "off" | "false" | "no")
}

pub fn settings_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join(APP_ID).join("settings.json")
}

impl Config {
    /// Settings file + API key. Cheap enough to call before every recording,
    /// so changes in the settings window apply immediately.
    pub fn load() -> Self {
        let path = settings_path();
        let mut cfg = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                crate::log::warn(&format!("bad settings file {}: {e}", path.display()));
                Self::default()
            }),
            Err(_) => {
                let cfg = Self::from_env();
                let _ = cfg.save();
                cfg
            }
        };
        cfg.api_key = api_key::get();
        cfg
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    /// Defaults overridden by MY_WHISPER_* variables (first run only).
    fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let d = Self::default();
        Self {
            api_key: String::new(),
            model: var("MY_WHISPER_MODEL").unwrap_or(d.model),
            language: std::env::var("MY_WHISPER_LANG").unwrap_or(d.language),
            microphone: var("MY_WHISPER_MIC").unwrap_or_default(),
            paste_key: var("MY_WHISPER_PASTE_KEY").map(|v| PasteKey::parse(&v)).unwrap_or(d.paste_key),
            cleanup: var("MY_WHISPER_CLEANUP").map(|v| parse_bool(&v)).unwrap_or(d.cleanup),
            cleanup_model: var("MY_WHISPER_CLEANUP_MODEL").unwrap_or(d.cleanup_model),
            vocabulary: var("MY_WHISPER_VOCAB").unwrap_or_default(),
            keep_audio: var("MY_WHISPER_KEEP_AUDIO").map(|v| parse_bool(&v)).unwrap_or(false),
            sounds: d.sounds,
        }
    }

    pub fn vocabulary_words(&self) -> Vec<&str> {
        self.vocabulary.split([',', '،', '\n']).map(str::trim).filter(|w| !w.is_empty()).collect()
    }

    /// Spelling hint sent to the speech model. A general "write English in
    /// Latin letters" instruction did not work reliably there (the cleanup
    /// step does that), so only the user's vocabulary is sent.
    pub fn stt_prompt(&self) -> Option<String> {
        let words = self.vocabulary_words();
        (!words.is_empty()).then(|| format!("واژه‌های پرکاربرد: {}.", words.join("، ")))
    }
}

/// The OpenRouter key in the OS keyring, cached so recordings don't hit
/// D-Bus every time. Falls back to `OPENROUTER_API_KEY` (dev `.env`).
pub mod api_key {
    use super::*;

    static CACHE: Mutex<Option<String>> = Mutex::new(None);

    fn entry() -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| format!("Keyring: {e}"))
    }

    pub fn get() -> String {
        if let Some(k) = CACHE.lock().unwrap().clone() {
            return k;
        }
        let stored = entry()
            .and_then(|e| e.get_password().map_err(|e| e.to_string()))
            .ok()
            .filter(|k| !k.trim().is_empty());
        let key = match stored {
            Some(k) => k,
            None => {
                // A key from `.env` (dev setup): move it into the keyring so
                // release builds, which don't read `.env`, find it too.
                let env_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
                if !env_key.trim().is_empty() {
                    if let Ok(e) = entry() {
                        let _ = e.set_password(env_key.trim());
                    }
                }
                env_key
            }
        };
        *CACHE.lock().unwrap() = Some(key.clone());
        key
    }

    /// Empty `key` removes it.
    pub fn set(key: &str) -> Result<(), String> {
        let key = key.trim();
        let e = entry()?;
        if key.is_empty() {
            match e.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(err) => return Err(format!("Keyring: {err}")),
            }
        } else {
            e.set_password(key).map_err(|err| format!("Keyring: {err}"))?;
        }
        *CACHE.lock().unwrap() = None;
        Ok(())
    }

    /// "sk-or-v1-…a1b2" for display.
    pub fn hint(key: &str) -> String {
        let k = key.trim();
        if k.len() <= 12 {
            return if k.is_empty() { String::new() } else { "…".into() };
        }
        format!("{}…{}", &k[..9], &k[k.len() - 4..])
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
        Config { vocabulary: vocabulary.into(), ..Config::default() }
    }

    #[test]
    fn empty_vocabulary_sends_no_prompt() {
        assert_eq!(cfg(" , ").stt_prompt(), None);
    }

    #[test]
    fn vocabulary_splits_on_commas_and_lines() {
        assert_eq!(cfg("Tauri, my-whisper،هادی\nDocker").vocabulary_words(), ["Tauri", "my-whisper", "هادی", "Docker"]);
    }

    #[test]
    fn parses_switch_values() {
        assert!(!parse_bool("off") && !parse_bool("0") && !parse_bool("False"));
        assert!(parse_bool("on") && parse_bool("1"));
    }

    #[test]
    fn settings_round_trip_without_key() {
        let c = Config { api_key: "secret".into(), cleanup: false, ..Config::default() };
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("secret"));
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Config { api_key: String::new(), ..c });
    }

    #[test]
    fn missing_fields_use_defaults() {
        let c: Config = serde_json::from_str(r#"{"model":"x"}"#).unwrap();
        assert_eq!(c.model, "x");
        assert!(c.cleanup);
    }

    #[test]
    fn key_hint_hides_middle() {
        assert_eq!(api_key::hint("sk-or-v1-0123456789abcdef"), "sk-or-v1-…cdef");
        assert_eq!(api_key::hint(""), "");
    }
}
