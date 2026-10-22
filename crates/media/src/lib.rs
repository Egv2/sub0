//! Wrappers around the external tools (ffmpeg, ffprobe, yt-dlp) plus VAD.
//! Everything goes through [`util::run_tool`], which sets `kill_on_drop` so a
//! cancelled job never leaves an orphaned ffmpeg behind.

pub mod chunk;
pub mod error;
pub mod ffmpeg;
pub mod ffprobe;
pub mod util;
pub mod vad;
pub mod ytdlp;

pub use chunk::{plan_chunks, ChunkPlan};
pub use error::MediaError;
pub use ffmpeg::{
    detect_silences, extract_audio, extract_audio_slice, AudioExtractFormat, Silence,
};
pub use ffprobe::{probe, MediaInfo};
pub use vad::{detect_speech, SpeechRange, VadParams};
pub use ytdlp::{
    classify_failure, download, expand_playlist, failure_class, parse_flat_playlist,
    parse_progress, version, DownloadedVideo, FailureClass,
};
