//! HTML preview page. Data is inlined so it opens from `file://` without a server.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;
use subtitler_core::model::{CueFlag, Track, TrackFlag, Word};
use subtitler_export::{break_lines, Layout};

#[derive(Debug, Serialize)]
pub struct PreviewCue {
    pub id: u32,
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
    /// Pre-broken with the export rules so the page doesn't reimplement them.
    pub lines: Vec<String>,
    pub flags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PreviewTrack {
    pub key: String,
    pub label: String,
    pub model: String,
    pub mode: String,
    pub cues: Vec<PreviewCue>,
    /// Empty for translated tracks.
    pub words: Vec<Word>,
    pub runs: Vec<serde_json::Value>,
    pub langs: Vec<String>,
}

#[derive(Serialize)]
struct PreviewData<'a> {
    video: String,
    input: String,
    providers: &'a [PreviewTrack],
}

pub fn flag_name(flag: &CueFlag) -> String {
    match flag {
        CueFlag::TooFast { cps } => format!("too_fast({cps:.1} cps)"),
        CueFlag::TooLong => "too_long".into(),
        CueFlag::TooShort => "too_short".into(),
        CueFlag::LineOverflow => "line_overflow".into(),
        CueFlag::TranslationRetried => "translation_retried".into(),
        CueFlag::TranslationFailed => "translation_failed".into(),
    }
}

pub fn track_from(
    index: usize,
    label: &str,
    model: &str,
    mode: &str,
    track: &Track,
    words: Vec<Word>,
    layout: Layout,
) -> PreviewTrack {
    let cues = track
        .cues
        .iter()
        .map(|cue| PreviewCue {
            id: cue.id,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: cue.text.clone(),
            lines: break_lines(&cue.text, layout),
            flags: track
                .flags
                .iter()
                .filter(|f: &&TrackFlag| f.cue_id == cue.id)
                .map(|f| flag_name(&f.flag))
                .collect(),
        })
        .collect();
    PreviewTrack {
        key: format!("t{index}"),
        label: label.to_string(),
        model: model.to_string(),
        mode: mode.to_string(),
        cues,
        words,
        runs: Vec::new(),
        langs: Vec::new(),
    }
}

pub fn write_preview(out_dir: &Path, input: &Path, tracks: &[PreviewTrack]) -> Result<PathBuf> {
    let data = serde_json::to_string(&PreviewData {
        video: video_src(input, out_dir),
        input: input.display().to_string(),
        providers: tracks,
    })?;
    let html = include_str!("../assets/preview.html")
        .replace("__SPIKE_DATA__", &data)
        .replace("__DATA_KEY__", "providers");
    let path = out_dir.join("preview.html");
    std::fs::write(&path, html)?;
    Ok(path)
}

/// Only spaces are percent-encoded, which is enough for a local preview.
fn video_src(input: &Path, out: &Path) -> String {
    if input.is_absolute() {
        return format!("file://{}", input.to_string_lossy().replace(' ', "%20"));
    }
    let mut rel = PathBuf::new();
    for _ in out.components() {
        rel.push("..");
    }
    rel.join(input).to_string_lossy().into_owned()
}
