use async_trait::async_trait;

use crate::error::SttError;
use crate::types::{AudioChunk, TranscribeOptions, Transcription};

// async_trait keeps this object-safe for `Box<dyn SttProvider>`.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Used in cache keys, so it must stay stable.
    fn name(&self) -> &'static str;

    /// Returned times are relative to the chunk; callers add `offset_ms`.
    async fn transcribe_chunk(
        &self,
        chunk: &AudioChunk,
        opts: &TranscribeOptions,
    ) -> Result<Transcription, SttError>;
}
