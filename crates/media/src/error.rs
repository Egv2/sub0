//! Errors for sidecar invocation and parsing.

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("required tool `{0}` not found in PATH")]
    BinaryNotFound(String),
    #[error("`{binary}` failed (exit {code}):\n{stderr}")]
    ToolFailed {
        binary: String,
        code: i32,
        stderr: String,
    },
    #[error("failed to parse `{0}` output: {1}")]
    Parse(String, String),
    #[error("invalid time range: start_ms {start_ms} >= end_ms {end_ms}")]
    InvalidTimeRange { start_ms: u32, end_ms: u32 },
    #[error("VAD failed: {0}")]
    Vad(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
