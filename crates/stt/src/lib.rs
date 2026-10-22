//! Speech-to-text providers (Groq, OpenRouter). A provider turns one audio
//! chunk into word timestamps; it knows nothing about cues.

pub mod defenses;
pub mod error;
pub mod groq;
pub mod openrouter;
pub mod provider;
pub mod types;
pub mod verbose;

pub use defenses::{apply_defenses, DefenseReport, TrimRange};
pub use error::SttError;
pub use groq::GroqProvider;
pub use openrouter::OpenRouterProvider;
pub use provider::SttProvider;
pub use types::{
    shift_transcription_times, AudioChunk, AudioFormat, Segment, TranscribeOptions, Transcription,
};
