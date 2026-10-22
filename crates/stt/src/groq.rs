//! Groq Whisper via the OpenAI-compatible `/audio/transcriptions` endpoint.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::multipart::{Form, Part};
use reqwest::{Client, Response};

use crate::error::{map_error_response, SttError};
use crate::provider::SttProvider;
use crate::types::{AudioChunk, TranscribeOptions, Transcription};
use crate::verbose::VerboseJson;

pub const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct GroqProvider {
    http: Client,
    api_key: String,
    base_url: String,
}

impl GroqProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            http: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("valid reqwest client"),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// For tests or other OpenAI-compatible gateways.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    fn retry_after_ms(resp: &Response) -> Option<u64> {
        resp.headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|secs| secs.saturating_mul(1000))
    }
}

#[async_trait]
impl SttProvider for GroqProvider {
    fn name(&self) -> &'static str {
        "groq"
    }

    async fn transcribe_chunk(
        &self,
        chunk: &AudioChunk,
        opts: &TranscribeOptions,
    ) -> Result<Transcription, SttError> {
        let url = format!("{}/audio/transcriptions", self.base_url);
        let file_name = format!("chunk.{}", chunk.format.ext());
        let part = Part::bytes(chunk.bytes.clone())
            .file_name(file_name)
            .mime_str(chunk.format.mime())
            .map_err(|e| SttError::BadResponse {
                provider: "groq",
                message: e.to_string(),
            })?;

        // One `timestamp_granularities[]` part per value; word timestamps only
        // come back with `verbose_json`.
        let mut form = Form::new()
            .part("file", part)
            .text("model", opts.model.clone())
            .text("response_format", "verbose_json")
            .text("temperature", opts.temperature.to_string())
            .text("timestamp_granularities[]", "segment")
            .text("timestamp_granularities[]", "word");
        if let Some(lang) = &opts.language {
            form = form.text("language", lang.clone());
        }
        if let Some(prompt) = &opts.prompt {
            form = form.text("prompt", prompt.clone());
        }

        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| SttError::Http {
                provider: "groq",
                source: e,
            })?;

        let retry_after_ms = Self::retry_after_ms(&resp);
        let status = resp.status();
        let body = resp.text().await.map_err(|e| SttError::Http {
            provider: "groq",
            source: e,
        })?;
        if !status.is_success() {
            return Err(map_error_response(
                "groq",
                status.as_u16(),
                retry_after_ms,
                &body,
                &opts.model,
            ));
        }

        let parsed: VerboseJson =
            serde_json::from_str(&body).map_err(|e| SttError::BadResponse {
                provider: "groq",
                message: e.to_string(),
            })?;
        Ok(parsed.into_transcription())
    }
}
