//! Subtitle timing and layout limits, loaded from `profiles/{name}.toml`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub max_lines: u32,
    pub max_chars_per_line: u32,
    /// Characters per second.
    pub max_cps: f32,
    pub min_duration_ms: u32,
    pub max_duration_ms: u32,
    pub min_gap_ms: u32,
    /// A pause this long splits a sentence into two cues.
    pub pause_split_ms: u32,
}

impl Default for Profile {
    // Netflix-style: 833 ms is 20 frames and 83 ms two frames at 24 fps.
    // `pause_split_ms` is our own tuning, not from the spec.
    fn default() -> Self {
        Self {
            name: "netflix".into(),
            max_lines: 2,
            max_chars_per_line: 42,
            max_cps: 17.0,
            min_duration_ms: 833,
            max_duration_ms: 7000,
            min_gap_ms: 83,
            pause_split_ms: 500,
        }
    }
}


impl Profile {
    /// `profiles/{name}.toml`, or the built-in defaults if it's missing.
    pub fn resolve(name: &str) -> Profile {
        let path = std::path::Path::new("profiles").join(format!("{name}.toml"));
        Self::load(&path).unwrap_or_default()
    }

    pub fn load(path: &std::path::Path) -> Option<Profile> {
        let text = std::fs::read_to_string(path).ok()?;
        toml::from_str::<Profile>(&text).ok()
    }

    pub fn layout(&self) -> subtitler_export::Layout {
        subtitler_export::Layout {
            max_chars_per_line: self.max_chars_per_line as usize,
            max_lines: self.max_lines as usize,
        }
    }

    pub fn max_total_chars(&self) -> usize {
        (self.max_lines * self.max_chars_per_line) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_resolution_falls_back_to_defaults() {
        let unknown = Profile::resolve("bilinmeyen-profil");
        assert_eq!(unknown, Profile::default());
        assert_eq!(unknown.max_chars_per_line, 42);
        assert_eq!(unknown.max_lines, 2);
        assert_eq!(Profile::resolve("netflix"), Profile::default());
    }

    #[test]
    fn layout_mirrors_the_profile() {
        let p = Profile::default();
        let l = p.layout();
        assert_eq!(l.max_chars_per_line, p.max_chars_per_line as usize);
        assert_eq!(l.max_lines, p.max_lines as usize);
    }
}
