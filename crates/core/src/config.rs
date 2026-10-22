//! Settings shared by the CLI and the app.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Groq,
    OpenRouter,
}

/// Re-check against the provider model lists before changing these.
pub mod model_slugs {
    /// $0.04/h, WER ~12%
    pub const GROQ_STT_FAST: &str = "whisper-large-v3-turbo";
    /// $0.111/h, WER ~10.3%
    pub const GROQ_STT_QUALITY: &str = "whisper-large-v3";
    pub const OPENROUTER_STT_DEFAULT: &str = "openai/whisper-large-v3";
    pub const GROQ_LLM_CLEANUP: &str = "openai/gpt-oss-20b";
    pub const GROQ_LLM_TRANSLATE: &str = "openai/gpt-oss-120b";
}

/// Cheap multilingual OpenRouter models (all with ≥100k context).
pub mod llm_presets {
    pub const OPENROUTER: &[(&str, &str)] = &[
        ("deepseek-v41-flash", "deepseek/deepseek-v4.1-flash"),
        ("deepseek-v4-flash", "deepseek/deepseek-v4-flash-0731"),
        ("glm-53-flash", "z-ai/glm-5.3-flash"),
        ("glm-47-flash", "z-ai/glm-4.7-flash"),
        ("gemma-4-26b", "google/gemma-4-26b-a4b-it"),
        ("qwen38-27b", "qwen/qwen3.8-27b"),
        ("qwen3-30b-a3b", "qwen/qwen3-30b-a3b-instruct-2507"),
        ("free-router", "openrouter/free"),
    ];

    pub fn resolve_openrouter(name: &str) -> Option<&'static str> {
        OPENROUTER
            .iter()
            .find(|(preset, _)| *preset == name)
            .map(|(_, slug)| *slug)
    }
}

/// Whisper hallucination thresholds. Music hallucinations can come with
/// perfect metrics, which is what the VAD is for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DefenseThresholds {
    pub max_no_speech_prob: f32,
    pub min_avg_logprob: f32,
    /// High compression ratio = repetition loop.
    pub max_compression_ratio: f32,
    /// Whisper likes to loop the last segment near the end of a file.
    pub collapse_repeated: bool,
}

impl Default for DefenseThresholds {
    fn default() -> Self {
        Self {
            max_no_speech_prob: 0.6,
            min_avg_logprob: -1.0,
            max_compression_ratio: 2.4,
            collapse_repeated: true,
        }
    }
}

/// Words outside detected speech are dropped as hallucinations, and chunk
/// cuts land on non-speech gaps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct VadSettings {
    pub enabled: bool,
    pub threshold: f32,
    pub min_speech_ms: u32,
    pub min_silence_ms: u32,
    pub speech_pad_ms: u32,
    /// Whisper word timestamps drift by a few hundred ms.
    pub word_tolerance_ms: u32,
}

impl Default for VadSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.5,
            min_speech_ms: 250,
            min_silence_ms: 300,
            speech_pad_ms: 200,
            word_tolerance_ms: 300,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SttSettings {
    pub provider: ProviderKind,
    pub model: String,
    /// `None` = auto-detect.
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub temperature: f32,
    #[serde(default)]
    pub defenses: DefenseThresholds,
    #[serde(default)]
    pub vad: VadSettings,
}

impl SttSettings {
    /// Always resolve defaults through here. OpenRouter doesn't reject Groq's
    /// bare `whisper-large-v3` slug; it routes it to a model without large-audio
    /// support and answers 400 on every file.
    pub fn default_for(provider: ProviderKind) -> Self {
        Self {
            provider,
            model: Self::default_model_for(provider).into(),
            language: None,
            temperature: 0.0,
            defenses: DefenseThresholds::default(),
            vad: VadSettings::default(),
        }
    }

    pub fn default_model_for(provider: ProviderKind) -> &'static str {
        match provider {
            ProviderKind::Groq => model_slugs::GROQ_STT_QUALITY,
            ProviderKind::OpenRouter => model_slugs::OPENROUTER_STT_DEFAULT,
        }
    }

    /// Catches a stale slug left over from a provider switch. Unknown but
    /// well-formed slugs pass; the provider decides what exists.
    pub fn is_model_valid_for(provider: ProviderKind, model: &str) -> bool {
        let model = model.trim();
        if model.is_empty() {
            return false;
        }
        match provider {
            ProviderKind::Groq => !model.contains('/'),
            ProviderKind::OpenRouter => model
                .split_once('/')
                .is_some_and(|(vendor, rest)| !vendor.trim().is_empty() && !rest.trim().is_empty()),
        }
    }
}

impl Default for SttSettings {
    fn default() -> Self {
        // large-v3 over turbo: turbo transcripts came out visibly truncated.
        Self::default_for(ProviderKind::Groq)
    }
}

/// `Pro` adds a localization brief, matching style profiles and the
/// already-translated previous cues on top of `Fast`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TranslationMode {
    Fast,
    #[default]
    Pro,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LlmSettings {
    pub provider: ProviderKind,
    pub model: String,
    #[serde(default)]
    pub temperature: f32,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Groq,
            model: model_slugs::GROQ_LLM_TRANSLATE.into(),
            temperature: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&ProviderKind::OpenRouter).unwrap(),
            r#""openrouter""#
        );
        assert_eq!(
            serde_json::from_str::<ProviderKind>(r#""groq""#).unwrap(),
            ProviderKind::Groq
        );
    }

    #[test]
    fn stt_settings_defaults_and_json_roundtrip() {
        let s = SttSettings::default();
        assert_eq!(s.provider, ProviderKind::Groq);
        assert_eq!(s.model, model_slugs::GROQ_STT_QUALITY);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<SttSettings>(&json).unwrap(), s);
    }

    /// OpenRouter answers 400 for Groq's bare slug, so defaults must differ.
    #[test]
    fn stt_defaults_are_provider_specific() {
        for (provider, expected) in [
            (ProviderKind::Groq, model_slugs::GROQ_STT_QUALITY),
            (
                ProviderKind::OpenRouter,
                model_slugs::OPENROUTER_STT_DEFAULT,
            ),
        ] {
            let s = SttSettings::default_for(provider);
            assert_eq!(s.provider, provider);
            assert_eq!(s.model, expected);
            assert!(
                SttSettings::is_model_valid_for(provider, &s.model),
                "{provider:?} default {expected:?} must be valid for its own provider"
            );
        }
        assert_ne!(
            SttSettings::default_model_for(ProviderKind::Groq),
            SttSettings::default_model_for(ProviderKind::OpenRouter),
            "the two providers must not share one default slug"
        );
    }

    #[test]
    fn cross_provider_slug_is_rejected() {
        let bare = model_slugs::GROQ_STT_QUALITY;
        assert!(!SttSettings::is_model_valid_for(
            ProviderKind::OpenRouter,
            bare
        ));
        assert!(!SttSettings::is_model_valid_for(
            ProviderKind::Groq,
            model_slugs::OPENROUTER_STT_DEFAULT
        ));
        assert!(SttSettings::is_model_valid_for(
            ProviderKind::OpenRouter,
            "openai/whisper-large-v3-turbo"
        ));
        assert!(SttSettings::is_model_valid_for(
            ProviderKind::Groq,
            "whisper-large-v3-turbo"
        ));
        assert!(!SttSettings::is_model_valid_for(ProviderKind::Groq, "  "));
        assert!(!SttSettings::is_model_valid_for(
            ProviderKind::OpenRouter,
            "/whisper-large-v3"
        ));
        assert!(!SttSettings::is_model_valid_for(
            ProviderKind::OpenRouter,
            "openai/"
        ));
    }
}
