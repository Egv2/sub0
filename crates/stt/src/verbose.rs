//! `verbose_json` parsing shared by Groq and OpenRouter. Everything is optional
//! because providers disagree on which fields they fill.

use serde::Deserialize;

use subtitler_core::time::secs_to_ms;

use crate::types::{Segment, Transcription};

#[derive(Debug, Clone, Deserialize)]
pub struct VerboseJson {
    #[serde(default)]
    pub text: String,
    pub language: Option<String>,
    pub duration: Option<f64>,
    #[serde(default)]
    pub words: Vec<JsonWord>,
    #[serde(default)]
    pub segments: Vec<JsonSegment>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonWord {
    pub word: String,
    pub start: f64,
    pub end: f64,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonSegment {
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub no_speech_prob: f32,
    #[serde(default)]
    pub avg_logprob: f32,
    #[serde(default)]
    pub compression_ratio: f32,
}

impl VerboseJson {
    pub fn into_transcription(self) -> Transcription {
        // OpenRouter sends words with a leading space (" world").
        let words = self
            .words
            .into_iter()
            .map(|w| subtitler_core::model::Word {
                text: w.word.trim().to_string(),
                start_ms: secs_to_ms(w.start),
                end_ms: secs_to_ms(w.end),
                prob: w.confidence,
            })
            .collect();
        let segments = self
            .segments
            .into_iter()
            .map(|s| Segment {
                start_ms: secs_to_ms(s.start),
                end_ms: secs_to_ms(s.end),
                text: s.text.trim().to_string(),
                no_speech_prob: s.no_speech_prob,
                avg_logprob: s.avg_logprob,
                compression_ratio: s.compression_ratio,
            })
            .collect();
        Transcription {
            text: self.text,
            language: self.language,
            duration_ms: self.duration.map(secs_to_ms),
            segments,
            words,
        }
    }
}

pub(crate) fn truncate_body(body: &str) -> String {
    const MAX: usize = 500;
    let body = body.trim();
    if body.len() <= MAX {
        body.to_string()
    } else {
        let mut cut = MAX;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}\u{2026}", &body[..cut])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_groq_verbose_json() {
        let body = r#"{
            "task": "transcribe",
            "language": "en",
            "duration": 10.31,
            "text": "Hello world.",
            "words": [
                { "word": "Hello", "start": 0.0, "end": 0.42 },
                { "word": "world.", "start": 0.5, "end": 0.9 }
            ],
            "segments": [
                {
                    "id": 0, "seek": 0, "start": 0.0, "end": 10.31,
                    "text": " Hello world.", "tokens": [1, 2],
                    "temperature": 0.0, "avg_logprob": -0.31,
                    "compression_ratio": 1.2, "no_speech_prob": 0.011
                }
            ]
        }"#;
        let parsed: VerboseJson = serde_json::from_str(body).unwrap();
        let t = parsed.into_transcription();
        assert_eq!(t.language.as_deref(), Some("en"));
        assert_eq!(t.duration_ms, Some(10310));
        assert_eq!(t.words.len(), 2);
        assert_eq!(t.words[0].start_ms, 0);
        assert_eq!(t.words[1].end_ms, 900);
        assert!(t.words[0].prob.is_none(), "groq words carry no confidence");
        assert_eq!(t.segments.len(), 1);
        assert_eq!(t.segments[0].no_speech_prob, 0.011);
        assert_eq!(t.segments[0].avg_logprob, -0.31);
    }

    #[test]
    fn parses_openrouter_verbose_json() {
        let body = r#"{
            "text": "Merhaba dünya.",
            "language": "tr",
            "duration": 3.2,
            "confidence": 0.93,
            "words": [
                { "word": "Merhaba", "start": 0.1, "end": 0.7, "confidence": 0.95 }
            ],
            "segments": [
                { "start": 0.0, "end": 3.2, "text": "Merhaba dünya." }
            ]
        }"#;
        let parsed: VerboseJson = serde_json::from_str(body).unwrap();
        let t = parsed.into_transcription();
        assert_eq!(t.words[0].prob, Some(0.95));
        assert_eq!(t.segments[0].no_speech_prob, 0.0, "missing metrics default");
    }

    #[test]
    fn missing_granularities_do_not_fail_parsing() {
        let parsed: VerboseJson = serde_json::from_str(r#"{ "text": "plain" }"#).unwrap();
        let t = parsed.into_transcription();
        assert!(t.words.is_empty());
        assert!(t.segments.is_empty());
        assert_eq!(t.text, "plain");
    }

    #[test]
    fn provider_whitespace_is_trimmed() {
        let body = r#"{
            "text": " padded.",
            "words": [{ "word": " world", "start": 0.0, "end": 0.5 }],
            "segments": [{ "start": 0.0, "end": 1.0, "text": " padded." }]
        }"#;
        let t: Transcription = serde_json::from_str::<VerboseJson>(body)
            .unwrap()
            .into_transcription();
        assert_eq!(t.words[0].text, "world");
        assert_eq!(t.segments[0].text, "padded.");
    }
}
