//! Speech-to-text providers.
//!
//! `SpeechToText` keeps the rest of the app independent of the provider, so
//! adding another service (or a local model) means adding one more impl.

use std::time::Duration;

use base64::Engine as _;
use serde::Deserialize;
use serde_json::json;

/// Upstream providers give up after about 60 s, so wait a little longer than that.
const TIMEOUT: Duration = Duration::from_secs(75);

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    /// USD, when the provider reports it.
    pub cost: Option<f64>,
}

pub trait SpeechToText: Send + Sync {
    /// `format` is the audio container, e.g. "wav".
    fn transcribe(&self, audio: &[u8], format: &str) -> Result<Transcript, String>;
}

pub struct OpenRouter {
    pub api_key: String,
    pub model: String,
    /// ISO-639-1 code such as "fa"; `None` lets the model detect it.
    pub language: Option<String>,
    pub base_url: String,
}

impl OpenRouter {
    pub const DEFAULT_BASE_URL: &'static str = "https://openrouter.ai/api/v1";

    pub fn new(api_key: String, model: String, language: Option<String>) -> Self {
        Self { api_key, model, language, base_url: Self::DEFAULT_BASE_URL.into() }
    }
}

#[derive(Deserialize)]
struct SuccessBody {
    text: String,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    cost: Option<f64>,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: Option<ErrorDetail>,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: Option<String>,
}

impl SpeechToText for OpenRouter {
    fn transcribe(&self, audio: &[u8], format: &str) -> Result<Transcript, String> {
        if self.api_key.trim().is_empty() {
            return Err("No API key set".into());
        }

        let mut body = json!({
            "model": self.model,
            "input_audio": {
                "data": base64::engine::general_purpose::STANDARD.encode(audio),
                "format": format,
            },
            "temperature": 0,
        });
        if let Some(lang) = self.language.as_deref().filter(|l| !l.is_empty()) {
            body["language"] = json!(lang);
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| e.to_string())?;

        let resp = client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .bearer_auth(self.api_key.trim())
            .header("X-Title", "my-whisper")
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    "Request timed out".to_string()
                } else if e.is_connect() {
                    "No connection to OpenRouter".to_string()
                } else {
                    format!("Network error: {e}")
                }
            })?;

        let status = resp.status();
        let text = resp.text().map_err(|e| e.to_string())?;

        if !status.is_success() {
            let detail = serde_json::from_str::<ErrorBody>(&text)
                .ok()
                .and_then(|b| b.error)
                .and_then(|e| e.message)
                .unwrap_or_else(|| text.chars().take(200).collect());
            eprintln!("my-whisper: OpenRouter {status}: {detail}");
            return Err(match status.as_u16() {
                401 => "Invalid API key".into(),
                402 => "Out of OpenRouter credits".into(),
                429 => "Rate limited, try again".into(),
                _ => format!("API error {}: {detail}", status.as_u16()),
            });
        }

        let parsed: SuccessBody =
            serde_json::from_str(&text).map_err(|e| format!("Unexpected response: {e}"))?;
        Ok(Transcript { text: parsed.text, cost: parsed.usage.and_then(|u| u.cost) })
    }
}
