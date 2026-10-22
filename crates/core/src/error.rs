#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("invalid time range: start_ms {start_ms} >= end_ms {end_ms}")]
    InvalidTimeRange { start_ms: u32, end_ms: u32 },
    #[error("invalid word data: {0}")]
    InvalidWords(String),
}
