//! Job stages and events. Transitions live in the pipeline crate.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::CueFlag;

/// SQLite row id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct JobId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStage {
    Queued,
    Downloading,
    Extracting,
    Chunking,
    Transcribing,
    Cleaning,
    Segmenting,
    Translating,
    Validating,
    Exporting,
    Done,
    Failed,
    Cancelled,
}

/// Broadcast to the CLI (printed) and the Tauri frontend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    StageStarted {
        job: JobId,
        stage: JobStage,
    },
    Progress {
        job: JobId,
        stage: JobStage,
        done: u32,
        total: u32,
    },
    CueFlagged {
        job: JobId,
        cue_id: u32,
        flag: CueFlag,
    },
    Warning {
        job: JobId,
        message: String,
    },
    Cost {
        job: JobId,
        tokens_in: u64,
        tokens_out: u64,
        usd: Option<f64>,
    },
    StageFinished {
        job: JobId,
        stage: JobStage,
    },
    JobFailed {
        job: JobId,
        error: String,
        retryable: bool,
    },
    JobDone {
        job: JobId,
        outputs: Vec<String>,
    },
    /// The source track is untouched, so these can be re-translated without STT.
    TranslationRemoved {
        job: JobId,
        langs: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_stage_roundtrips_snake_case() {
        let json = serde_json::to_string(&JobStage::Downloading).unwrap();
        assert_eq!(json, r#""downloading""#);
        assert_eq!(
            serde_json::from_str::<JobStage>(&json).unwrap(),
            JobStage::Downloading
        );
    }

    #[test]
    fn job_event_roundtrips() {
        let event = JobEvent::Progress {
            job: JobId(42),
            stage: JobStage::Transcribing,
            done: 3,
            total: 10,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"progress""#));
        assert_eq!(serde_json::from_str::<JobEvent>(&json).unwrap(), event);
    }

    #[test]
    fn job_event_schema_generates() {
        let schema = serde_json::to_string(&schemars::schema_for!(JobEvent)).unwrap();
        assert!(!schema.is_empty());
    }
}
