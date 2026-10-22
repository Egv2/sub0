//! Deterministic words-to-cues segmentation. Limit violations that can't be
//! fixed are flagged, not silently patched.

pub mod profile;
pub mod segmenter;

pub use profile::Profile;
pub use segmenter::segment;

#[cfg(test)]
mod tests;
