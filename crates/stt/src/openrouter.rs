//! OpenRouter transcription. Sends audio as base64 JSON rather than multipart,
//! which avoids the 25 MB multipart cap. Upstream providers time out after
//! about 60 s, so chunks must stay short.

use std::time::Duration;

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::{Client, Response};
use serde_json::json;

use crate::error::{map_error_response, SttError};
use crate::provider::SttProvider;
use crate::types::{AudioChunk, TranscribeOptions, Transcription};
use crate::verbose::VerboseJson;

pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct OpenRouterProvider {
    http: Client,
    api_key: String,
    base_url: String,
}

impl OpenRouterProvider {
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
impl SttProvider for OpenRouterProvider {
    fn name(&self) -> &'static str {
        "openrouter"
    }

    async fn transcribe_chunk(
        &self,
        chunk: &AudioChunk,
        opts: &TranscribeOptions,
    ) -> Result<Transcription, SttError> {
        let url = format!("{}/audio/transcriptions", self.base_url);
        // `AudioFormat::ext()` matches OpenRouter's accepted `format` values.
        let mut body = json!({
            "model": opts.model,
            "input_audio": {
                "data": STANDARD.encode(&chunk.bytes),
                "format": chunk.format.ext(),
            },
            "response_format": "verbose_json",
            "timestamp_granularities": ["segment", "word"],
            "temperature": opts.temperature,
        });
        if let Some(lang) = &opts.language {
            body["language"] = json!(lang);
        }

        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| SttError::Http {
                provider: "openrouter",
                source: e,
            })?;

        let retry_after_ms = Self::retry_after_ms(&resp);
        let status = resp.status();
        let body = resp.text().await.map_err(|e| SttError::Http {
            provider: "openrouter",
            source: e,
        })?;
        if !status.is_success() {
            return Err(map_error_response(
                "openrouter",
                status.as_u16(),
                retry_after_ms,
                &body,
                &opts.model,
            ));
        }

        let parsed: VerboseJson =
            serde_json::from_str(&body).map_err(|e| SttError::BadResponse {
                provider: "openrouter",
                message: e.to_string(),
            })?;
        Ok(parsed.into_transcription())
    }
}
