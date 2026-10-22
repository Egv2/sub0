use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Word {
    pub text: String,
    pub start_ms: u32,
    pub end_ms: u32,
    pub prob: Option<f32>,
}

/// `text` is always a single line; line breaking happens at export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cue {
    pub id: u32,
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
}

impl Cue {
    /// Ids are assigned once at segmentation; translations reuse them.
    pub fn new(
        id: u32,
        start_ms: u32,
        end_ms: u32,
        text: impl Into<String>,
    ) -> Result<Self, CoreError> {
        if start_ms >= end_ms {
            return Err(CoreError::InvalidTimeRange { start_ms, end_ms });
        }
        Ok(Self {
            id,
            start_ms,
            end_ms,
            text: text.into(),
        })
    }

    pub fn duration_ms(&self) -> u32 {
        self.end_ms - self.start_ms
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CueFlag {
    TooFast { cps: f32 },
    TooLong,
    TooShort,
    LineOverflow,
    TranslationRetried,
    TranslationFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrackFlag {
    pub cue_id: u32,
    pub flag: CueFlag,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Track {
    pub lang: String,
    pub cues: Vec<Cue>,
    pub flags: Vec<TrackFlag>,
}

impl Track {
    pub fn timecode_fingerprint(&self) -> Vec<(u32, u32, u32)> {
        self.cues
            .iter()
            .map(|c| (c.id, c.start_ms, c.end_ms))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
pub enum TimecodeMismatch {
    #[error("cue count differs: source has {source_count}, translated has {translated_count}")]
    Count {
        source_count: usize,
        translated_count: usize,
    },
    #[error(
        "cue index {index} timecodes differ: source {source_ms:?} ms, translated {translated_ms:?} ms"
    )]
    Cue {
        index: usize,
        source_ms: (u32, u32, u32),
        translated_ms: (u32, u32, u32),
    },
}

/// A translation must keep the source's exact `(id, start_ms, end_ms)` sequence.
/// Also checked at export time, not just in tests.
pub fn assert_timecodes_preserved(
    source: &Track,
    translated: &Track,
) -> Result<(), TimecodeMismatch> {
    let a = source.timecode_fingerprint();
    let b = translated.timecode_fingerprint();
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            return Err(TimecodeMismatch::Cue {
                index: i,
                source_ms: *x,
                translated_ms: *y,
            });
        }
    }
    if a.len() != b.len() {
        return Err(TimecodeMismatch::Count {
            source_count: a.len(),
            translated_count: b.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(id: u32, start: u32, end: u32, text: &str) -> Cue {
        Cue::new(id, start, end, text).unwrap()
    }

    fn source_track() -> Track {
        Track {
            lang: "en".into(),
            cues: vec![cue(0, 0, 2000, "hello"), cue(1, 2500, 4000, "world")],
            flags: vec![],
        }
    }

    #[test]
    fn cue_rejects_empty_range() {
        assert!(Cue::new(0, 100, 100, "x").is_err());
        assert!(Cue::new(0, 200, 100, "x").is_err());
    }

    #[test]
    fn word_and_cue_roundtrip_through_json() {
        let word = Word {
            text: "so".into(),
            start_ms: 10,
            end_ms: 200,
            prob: Some(0.9),
        };
        let json = serde_json::to_string(&word).unwrap();
        assert_eq!(serde_json::from_str::<Word>(&json).unwrap(), word);

        let cue = cue(7, 1000, 3500, "hi there");
        let json = serde_json::to_string(&cue).unwrap();
        assert_eq!(serde_json::from_str::<Cue>(&json).unwrap(), cue);
    }

    #[test]
    fn cueflag_is_tagged_and_roundtrips() {
        let flag = CueFlag::TooFast { cps: 21.5 };
        let json = serde_json::to_string(&flag).unwrap();
        assert!(json.contains(r#""type":"too_fast""#));
        assert_eq!(serde_json::from_str::<CueFlag>(&json).unwrap(), flag);
    }

    #[test]
    fn schemars_generates_schemas() {
        for schema in [
            serde_json::to_string(&schemars::schema_for!(Word)).unwrap(),
            serde_json::to_string(&schemars::schema_for!(Track)).unwrap(),
            serde_json::to_string(&schemars::schema_for!(CueFlag)).unwrap(),
        ] {
            assert!(!schema.is_empty());
        }
    }

    #[test]
    fn timecode_preservation_accepts_identical_timing() {
        let mut translated = source_track();
        translated.lang = "de".into();
        translated.cues[0].text = "hallo".into();
        translated.cues[1].text = "welt".into();
        assert_eq!(
            assert_timecodes_preserved(&source_track(), &translated),
            Ok(())
        );
    }

    #[test]
    fn timecode_preservation_rejects_drifted_start() {
        let mut translated = source_track();
        translated.cues[1].start_ms = 2600;
        assert!(assert_timecodes_preserved(&source_track(), &translated).is_err());
    }

    #[test]
    fn timecode_preservation_rejects_dropped_and_added_cues() {
        let mut dropped = source_track();
        // Dropping the last cue (not the first) keeps prefixes aligned, so
        // we hit the count check rather than a Cue mismatch.
        dropped.cues.pop();
        assert!(matches!(
            assert_timecodes_preserved(&source_track(), &dropped),
            Err(TimecodeMismatch::Count { .. })
        ));

        let mut added = source_track();
        added.cues.push(cue(2, 5000, 6000, "extra"));
        assert!(matches!(
            assert_timecodes_preserved(&source_track(), &added),
            Err(TimecodeMismatch::Count { .. })
        ));
    }

    #[test]
    fn timecode_preservation_rejects_reorder() {
        let mut reordered = source_track();
        reordered.cues.swap(0, 1);
        assert!(matches!(
            assert_timecodes_preserved(&source_track(), &reordered),
            Err(TimecodeMismatch::Cue { .. })
        ));
    }
}
