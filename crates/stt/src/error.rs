
#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("missing API key for provider `{0}` (set it in settings/keychain or the environment)")]
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
    /// OpenRouter answers a wrong slug (e.g. bare `whisper-large-v3` instead of
    /// `openai/whisper-large-v3`) with a 400 "does not support large audio inputs",
    /// which looks like a chunking problem but isn't.
    #[error(
        "model `{model}` cannot transcribe audio on {provider}: {detail} \
         (OpenRouter slugs must be `vendor/model`, e.g. `openai/whisper-large-v3`)"
    )]
    UnsupportedModel {
        provider: &'static str,
        model: String,
        detail: String,
    },
}

const OPENROUTER_UNSUPPORTED_AUDIO: &str = "does not support large audio inputs";

pub(crate) fn is_unsupported_model_message(body: &str) -> bool {
    body.contains(OPENROUTER_UNSUPPORTED_AUDIO)
}

pub(crate) fn map_error_response(
    provider: &'static str,
    status: u16,
    retry_after_ms: Option<u64>,
    body: &str,
    model: &str,
) -> SttError {
    if status == 429 {
        return SttError::RateLimited {
            provider,
            retry_after_ms,
        };
    }
    let message = crate::verbose::truncate_body(body);
    if is_unsupported_model_message(body) {
        return SttError::UnsupportedModel {
            provider,
            model: model.to_string(),
            detail: message,
        };
    }
    SttError::Api {
        provider,
        status,
        message,
    }
}

impl SttError {
    pub fn is_transient(&self) -> bool {
        match self {
            SttError::RateLimited { .. } | SttError::Http { .. } => true,
            SttError::Api { status, .. } => *status >= 500,
            SttError::MissingKey(_)
            | SttError::BadResponse { .. }
            | SttError::UnsupportedModel { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPENROUTER_BODY: &str =
        r#"{"error":{"message":"The selected model does not support large audio inputs","code":400}}"#;

    #[test]
    fn openrouter_audio_refusal_becomes_unsupported_model() {
        let err = map_error_response("openrouter", 400, None, OPENROUTER_BODY, "whisper-large-v3");
        match &err {
            SttError::UnsupportedModel { model, .. } => assert_eq!(model, "whisper-large-v3"),
            other => panic!("expected UnsupportedModel, got {other:?}"),
        }
        assert!(!err.is_transient(), "retrying cannot fix a wrong slug");
        let text = err.to_string();
        assert!(text.contains("openai/whisper-large-v3"), "{text}");
    }

    #[test]
    fn rate_limit_and_plain_400_keep_their_own_variants() {
        assert!(matches!(
            map_error_response("groq", 429, Some(2000), "slow down", "m"),
            SttError::RateLimited { .. }
        ));
        let err = map_error_response("groq", 400, None, r#"{"error":"bad file"}"#, "m");
        assert!(matches!(err, SttError::Api { status: 400, .. }));
        assert!(!err.is_transient());
    }

    #[test]
    fn server_errors_stay_transient() {
        let err = map_error_response("openrouter", 503, None, "upstream down", "m");
        assert!(err.is_transient());
    }
}
