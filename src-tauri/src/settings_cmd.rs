//! Commands behind the settings window.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::audio::{self, Recorder};
use crate::config::{api_key, Config};

#[derive(Serialize)]
pub struct SettingsView {
    #[serde(flatten)]
    config: Config,
    has_api_key: bool,
    /// e.g. "sk-or-v1-…a1b2"
    api_key_hint: String,
    settings_path: String,
}

#[tauri::command]
pub fn get_settings() -> SettingsView {
    let config = Config::load();
    SettingsView {
        has_api_key: !config.api_key.trim().is_empty(),
        api_key_hint: api_key::hint(&config.api_key),
        settings_path: crate::config::settings_path().display().to_string(),
        config,
    }
}

#[tauri::command]
pub fn save_settings(settings: Config) -> Result<(), String> {
    settings.save()
}

/// Empty string removes the key.
#[tauri::command]
pub fn set_api_key(key: String) -> Result<(), String> {
    api_key::set(&key)
}

#[derive(Serialize)]
pub struct KeyInfo {
    /// USD spent with this key so far.
    usage: f64,
    usage_daily: f64,
    /// USD left when the key has a limit.
    limit_remaining: Option<f64>,
}

/// Checks a key against OpenRouter. Uses `key` if given, else the saved one.
#[tauri::command]
pub async fn test_api_key(key: Option<String>) -> Result<KeyInfo, String> {
    let key = key.filter(|k| !k.trim().is_empty()).unwrap_or_else(api_key::get);
    if key.trim().is_empty() {
        return Err("No API key set".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let resp = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?
            .get("https://openrouter.ai/api/v1/key")
            .bearer_auth(key.trim())
            .send()
            .map_err(|e| format!("No connection to OpenRouter: {e}"))?;
        if resp.status().as_u16() == 401 {
            return Err("Invalid API key".to_string());
        }
        if !resp.status().is_success() {
            return Err(format!("OpenRouter error {}", resp.status()));
        }
        let v: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
        let d = &v["data"];
        Ok(KeyInfo {
            usage: d["usage"].as_f64().unwrap_or(0.0),
            usage_daily: d["usage_daily"].as_f64().unwrap_or(0.0),
            limit_remaining: d["limit_remaining"].as_f64(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn list_microphones() -> Vec<audio::Microphone> {
    audio::list_microphones()
}

#[derive(Serialize)]
pub struct MicTest {
    peak: f32,
    speech_seconds: f32,
    silent: bool,
}

/// Records for `seconds`, emitting "mic-test-level" events for a live meter.
#[tauri::command]
pub async fn test_microphone(app: AppHandle, id: String, seconds: f32) -> Result<MicTest, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let level_app = app.clone();
        let rec = Recorder::start(Some(id.as_str()), move |l| {
            let _ = level_app.emit("mic-test-level", l);
        })?;
        std::thread::sleep(Duration::from_secs_f32(seconds.clamp(1.0, 10.0)));
        let r = rec.stop()?;
        Ok(MicTest {
            peak: r.samples.iter().fold(0f32, |m, s| m.max(s.abs())),
            speech_seconds: r.speech_seconds(),
            silent: r.is_silent(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
