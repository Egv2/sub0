use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use subtitler_core::model::Word;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Flac,
    Wav,
    Mp3,
    M4a,
    Ogg,
    Webm,
}

impl AudioFormat {
    pub fn ext(self) -> &'static str {
        match self {
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::M4a => "m4a",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Webm => "webm",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            AudioFormat::Flac => "audio/flac",
            AudioFormat::Wav => "audio/wav",
            AudioFormat::Mp3 => "audio/mpeg",
            AudioFormat::M4a => "audio/mp4",
            AudioFormat::Ogg => "audio/ogg",
            AudioFormat::Webm => "audio/webm",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub bytes: Vec<u8>,
    pub format: AudioFormat,
    /// Where this chunk starts in the source timeline.
    pub offset_ms: u32,
    pub duration_ms: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscribeOptions {
    pub model: String,
    /// `None` means auto-detect.
    pub language: Option<String>,
    pub temperature: f32,
    /// Groq honors this; OpenRouter ignores it.
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Segment {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
    #[serde(default)]
    pub no_speech_prob: f32,
    #[serde(default)]
    pub avg_logprob: f32,
    #[serde(default)]
    pub compression_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Transcription {
    pub text: String,
    pub language: Option<String>,
    pub duration_ms: Option<u32>,
    pub segments: Vec<Segment>,
    pub words: Vec<Word>,
}

/// Saturating, so a bogus provider timestamp can't panic the pipeline.
pub fn shift_transcription_times(t: &mut Transcription, offset_ms: u32) {
    for w in &mut t.words {
        w.start_ms = w.start_ms.saturating_add(offset_ms);
        w.end_ms = w.end_ms.saturating_add(offset_ms);
    }
    for s in &mut t.segments {
        s.start_ms = s.start_ms.saturating_add(offset_ms);
        s.end_ms = s.end_ms.saturating_add(offset_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_format_values_match_provider_docs() {
        assert_eq!(AudioFormat::Flac.ext(), "flac");
        assert_eq!(AudioFormat::Flac.mime(), "audio/flac");
        assert_eq!(AudioFormat::M4a.ext(), "m4a");
        assert_eq!(
            serde_json::to_string(&AudioFormat::Wav).unwrap(),
            r#""wav""#
        );
    }

    #[test]
    fn shifting_is_saturating_and_additive() {
        let mut t = Transcription {
            words: vec![Word {
                text: "hi".into(),
                start_ms: u32::MAX - 5,
                end_ms: u32::MAX - 1,
                prob: None,
            }],
            ..Default::default()
        };
        shift_transcription_times(&mut t, 100);
        assert_eq!(t.words[0].start_ms, u32::MAX);
        assert_eq!(t.words[0].end_ms, u32::MAX);

        let mut t = Transcription {
            words: vec![Word {
                text: "hi".into(),
                start_ms: 10,
                end_ms: 20,
                prob: None,
            }],
            ..Default::default()
        };
        shift_transcription_times(&mut t, 1000);
        assert_eq!((t.words[0].start_ms, t.words[0].end_ms), (1010, 1020));
    }
}
