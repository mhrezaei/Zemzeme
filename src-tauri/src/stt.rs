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
    /// Hint text for the model (spelling, vocabulary). Only some providers use it.
    pub prompt: Option<String>,
    pub base_url: String,
    pub timeout: Duration,
}

impl OpenRouter {
    pub const DEFAULT_BASE_URL: &'static str = "https://openrouter.ai/api/v1";

    pub fn new(api_key: String, model: String, language: Option<String>) -> Self {
        Self { api_key, model, language, prompt: None, base_url: Self::DEFAULT_BASE_URL.into(), timeout: TIMEOUT }
    }

    pub fn with_prompt(mut self, prompt: Option<String>) -> Self {
        self.prompt = prompt.filter(|p| !p.trim().is_empty());
        self
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
        // OpenRouter has no top-level prompt for transcription; it is passed
        // per provider. OpenAI and Groq both take a `prompt` field. Only the
        // serving provider's entry is forwarded, so listing both is safe.
        if let Some(prompt) = &self.prompt {
            body["provider"] = json!({
                "options": {
                    "openai": { "prompt": prompt },
                    "groq": { "prompt": prompt },
                }
            });
        }

        let client = reqwest::blocking::Client::builder().timeout(self.timeout).build().map_err(|e| e.to_string())?;

        let resp = client
            .post(format!("{}/audio/transcriptions", self.base_url))
            .bearer_auth(self.api_key.trim())
            .header("X-Title", "Zemzeme")
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
            crate::log::warn(&format!("OpenRouter {status}: {detail}"));
            return Err(match status.as_u16() {
                401 => "Invalid API key".into(),
                402 => "Out of OpenRouter credits".into(),
                429 => "Rate limited, try again".into(),
                _ => format!("API error {}: {detail}", status.as_u16()),
            });
        }

        let parsed: SuccessBody = serde_json::from_str(&text).map_err(|e| format!("Unexpected response: {e}"))?;
        Ok(Transcript { text: parsed.text, cost: parsed.usage.and_then(|u| u.cost) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::{closed_url, serve};

    fn client(base_url: String) -> OpenRouter {
        let mut c = OpenRouter::new("sk-test".into(), "openai/gpt-4o-transcribe".into(), Some("fa".into()));
        c.base_url = base_url;
        c.timeout = Duration::from_millis(500);
        c
    }

    fn fail_with(status: u16, body: &str) -> String {
        let (url, server) = serve(status, body, Duration::ZERO);
        let err = client(url).transcribe(b"RIFF", "wav").unwrap_err();
        server.join().unwrap();
        err
    }

    #[test]
    fn sends_audio_and_reads_text() {
        let (url, server) = serve(200, r#"{"text":"سلام","usage":{"cost":0.0004}}"#, Duration::ZERO);
        let c = client(url).with_prompt(Some("Docker".into()));
        let t = c.transcribe(b"RIFF", "wav").unwrap();
        assert_eq!(t.text, "سلام");
        assert_eq!(t.cost, Some(0.0004));

        let req = server.join().unwrap();
        assert!(req.head.starts_with("POST /audio/transcriptions"));
        assert!(req.head.to_ascii_lowercase().contains("authorization: bearer sk-test"));
        assert_eq!(req.body["model"], "openai/gpt-4o-transcribe");
        assert_eq!(req.body["language"], "fa");
        assert_eq!(req.body["input_audio"]["format"], "wav");
        assert_eq!(req.body["input_audio"]["data"], "UklGRg==");
        assert_eq!(req.body["provider"]["options"]["openai"]["prompt"], "Docker");
    }

    #[test]
    fn explains_common_errors() {
        assert_eq!(fail_with(401, r#"{"error":{"message":"No auth"}}"#), "Invalid API key");
        assert_eq!(fail_with(402, "{}"), "Out of OpenRouter credits");
        assert_eq!(fail_with(429, "{}"), "Rate limited, try again");
        assert_eq!(fail_with(500, r#"{"error":{"message":"boom"}}"#), "API error 500: boom");
        assert_eq!(fail_with(502, "Bad gateway"), "API error 502: Bad gateway");
    }

    #[test]
    fn rejects_unexpected_response() {
        assert!(fail_with(200, r#"{"nope":1}"#).starts_with("Unexpected response"));
    }

    #[test]
    fn reports_timeout() {
        let (url, server) = serve(200, r#"{"text":"late"}"#, Duration::from_secs(2));
        assert_eq!(client(url).transcribe(b"RIFF", "wav").unwrap_err(), "Request timed out");
        server.join().unwrap();
    }

    #[test]
    fn reports_no_connection() {
        assert_eq!(client(closed_url()).transcribe(b"RIFF", "wav").unwrap_err(), "No connection to OpenRouter");
    }

    #[test]
    fn needs_a_key() {
        let mut c = client(closed_url());
        c.api_key = "  ".into();
        assert_eq!(c.transcribe(b"RIFF", "wav").unwrap_err(), "No API key set");
    }
}
