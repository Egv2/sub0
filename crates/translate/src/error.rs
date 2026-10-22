#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("missing API key for provider `{0}`")]
    MissingKey(&'static str),
    #[error("HTTP request to {provider} failed: {source}")]
    Http {
        provider: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("rate limited by {provider} (Retry-After: {retry_after_ms:?} ms)")]
    RateLimited {
        provider: &'static str,
        retry_after_ms: Option<u64>,
    },
    #[error("{provider} API error {status}: {message}")]
    Api {
        provider: &'static str,
        status: u16,
        message: String,
    },
    #[error("{provider} returned an unexpected response: {message}")]
    BadResponse {
        provider: &'static str,
        message: String,
    },
}

impl LlmError {
    pub fn is_transient(&self) -> bool {
        matches!(self, LlmError::RateLimited { .. } | LlmError::Http { .. })
    }
}

pub(crate) fn map_error_response(
    provider: &'static str,
    status: u16,
    retry_after_ms: Option<u64>,
    body: &str,
) -> LlmError {
    if status == 429 {
        return LlmError::RateLimited {
            provider,
            retry_after_ms,
        };
    }
    let message = crate::translate::truncate_body(body);
    LlmError::Api {
        provider,
        status,
        message,
    }
}
