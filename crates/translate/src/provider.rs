//! Groq and OpenRouter both speak OpenAI-style chat completions, so one client covers both.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tokio::time::sleep;

use crate::error::{map_error_response, LlmError};

#[derive(Debug, Clone)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn chat_json(
        &self,
        model: &str,
        system: &str,
        user: &str,
        temperature: f32,
    ) -> Result<(String, Option<Usage>), LlmError>;
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatLlm {
    name: &'static str,
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenAiCompatLlm {
    pub fn groq(api_key: impl Into<String>) -> Self {
        Self::new("groq", "https://api.groq.com/openai/v1", api_key)
    }

    pub fn openrouter(api_key: impl Into<String>) -> Self {
        Self::new("openrouter", "https://openrouter.ai/api/v1", api_key)
    }

    pub fn new(name: &'static str, base_url: &str, api_key: impl Into<String>) -> Self {
        Self {
            name,
            // Long batch prompts can be slow; retries are handled by the caller.
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("valid reqwest client"),
            api_key: api_key.into(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    fn retry_after_ms(resp: &reqwest::Response) -> Option<u64> {
        resp.headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|secs| secs.saturating_mul(1000))
    }
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    #[serde(default)]
    message: Option<ChatMessage>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoiceMessage>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

#[async_trait]
impl LlmProvider for OpenAiCompatLlm {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn chat_json(
        &self,
        model: &str,
        system: &str,
        user: &str,
        temperature: f32,
    ) -> Result<(String, Option<Usage>), LlmError> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
            "temperature": temperature,
            // Reasoning models burn budget before emitting any JSON. Without
            // headroom, big batches come back truncated or empty.
            "max_tokens": 16384,
            "response_format": { "type": "json_object" },
        });
        // OpenRouter can route even flash models to a reasoning upstream that
        // returns content: null. Groq rejects unknown params, so OpenRouter only.
        if self.name == "openrouter" {
            body["reasoning"] = json!({ "enabled": false });
        }
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Http {
                provider: self.name,
                source: e,
            })?;

        let retry_after_ms = Self::retry_after_ms(&resp);
        let status = resp.status();
        let text = resp.text().await.map_err(|e| LlmError::Http {
            provider: self.name,
            source: e,
        })?;
        if !status.is_success() {
            return Err(map_error_response(
                self.name,
                status.as_u16(),
                retry_after_ms,
                &text,
            ));
        }

        let parsed: ChatResponse =
            serde_json::from_str(&text).map_err(|e| LlmError::BadResponse {
                provider: self.name,
                message: e.to_string(),
            })?;
        let (content, finish_reason) = parsed
            .choices
            .first()
            .map(|c| {
                (
                    c.message.as_ref().and_then(|m| m.content.clone()),
                    c.finish_reason.clone(),
                )
            })
            .unwrap_or_default();
        let content = content.ok_or_else(|| LlmError::BadResponse {
            provider: self.name,
            // Some gateways return 200 with an error body near quota limits.
            message: format!(
                "no choices[0].message.content (body: {})",
                crate::translate::truncate_body(&text)
            ),
        })?;
        if content.is_empty() {
            return Err(LlmError::BadResponse {
                provider: self.name,
                message: format!(
                    "empty content (finish_reason: {:?}) — often a token/quota limit on reasoning models",
                    finish_reason
                ),
            });
        }
        let usage = parsed.usage.map(|u| Usage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
        });
        Ok((content, usage))
    }
}

pub(crate) async fn backoff_ms(attempt: u32) {
    // 1s, 2s, 4s, then capped at 8s.
    let ms = 1000u64.saturating_mul(1u64 << attempt.min(3));
    sleep(Duration::from_millis(ms)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression: content lives under choices[0].message, not on the choice.
    // Mock providers never caught this since they skip the HTTP layer.
    #[test]
    fn parses_content_from_nested_message() {
        let body = r#"{
            "id": "gen-x", "object": "chat.completion",
            "model": "m", "provider": "Together",
            "choices": [{
                "index": 0, "logprobs": null,
                "finish_reason": "stop", "native_finish_reason": "stop",
                "message": { "role": "assistant", "content": "{\"translations\": []}" }
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 5 }
        }"#;
        let parsed: ChatResponse = serde_json::from_str(body).unwrap();
        let content = parsed.choices[0]
            .message
            .as_ref()
            .and_then(|m| m.content.clone())
            .expect("content must come from choices[0].message.content");
        assert!(content.contains("translations"));
        assert_eq!(parsed.choices[0].finish_reason.as_deref(), Some("stop"));
        let usage = parsed.usage.unwrap();
        assert_eq!((usage.prompt_tokens, usage.completion_tokens), (10, 5));
    }

    #[test]
    fn null_message_content_maps_to_none_not_parse_error() {
        let body = r#"{
            "choices": [{ "finish_reason": "length",
                          "message": { "role": "assistant", "content": null, "reasoning": "thinking..." } }]
        }"#;
        let parsed: ChatResponse = serde_json::from_str(body).unwrap();
        let content = parsed.choices[0]
            .message
            .as_ref()
            .and_then(|m| m.content.clone());
        assert!(
            content.is_none(),
            "null content stays None for the retry ladder"
        );
        assert_eq!(parsed.choices[0].finish_reason.as_deref(), Some("length"));
    }
}
