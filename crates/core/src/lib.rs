//! Shared domain types. Time is integer milliseconds everywhere; floats are
//! converted once at API boundaries via [`time::secs_to_ms`].

pub mod config;
pub mod error;
pub mod job;
pub mod lang;
pub mod model;
pub mod time;

pub use error::CoreError;
pub use lang::{by_code, is_supported, normalise_all, Language, LANGUAGES, DEFAULT_TARGET};
pub use model::{assert_timecodes_preserved, TimecodeMismatch};
