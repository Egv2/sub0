//! SRT, WebVTT and plain-text writers. Cue text is stored unbroken; line
//! breaking happens here, at export time.

use std::path::Path;

use subtitler_core::model::Track;
use subtitler_core::TimecodeMismatch;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub max_chars_per_line: usize,
    pub max_lines: usize,
}

/// Two-line cues are split bottom-heavy: the second line is the longer one.
pub fn break_lines(text: &str, layout: Layout) -> Vec<String> {
    if layout.max_lines <= 1 || text.graphemes(true).count() <= layout.max_chars_per_line {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 2 {
        return vec![text.to_string()];
    }
    // Break at the last space at or before the midpoint. Overlong cues get a
    // best-effort split; the segmenter already flagged them as LineOverflow.
    let total: usize = text.graphemes(true).count();
    let target = total / 2;
    let mut best: Option<usize> = None; // word index after which to break
    let mut pos = 0usize;
    for (i, word) in words.iter().enumerate() {
        if i + 1 == words.len() {
            break;
        }
        pos += word.graphemes(true).count() + 1; // +1 for the joining space
        let line1 = pos;
        let line2 = total - line1;
        let fits = line1 <= layout.max_chars_per_line && line2 <= layout.max_chars_per_line;
        if fits && line1 <= target {
            best = Some(i + 1);
        }
    }
    match best {
        Some(at) => {
            let (a, b) = words.split_at(at);
            vec![a.join(" "), b.join(" ")]
        }
        None => greedy_fill(&words, layout),
    }
}

fn greedy_fill(words: &[&str], layout: Layout) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words {
        let candidate_len = current.graphemes(true).count()
            + if current.is_empty() { 0 } else { 1 }
            + word.graphemes(true).count();
        if !current.is_empty() && candidate_len > layout.max_chars_per_line {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn srt_time(ms: u32) -> String {
    let h = ms / 3_600_000;
    let m = (ms / 60_000) % 60;
    let s = (ms / 1000) % 60;
    let msec = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{msec:03}")
}

fn vtt_time(ms: u32) -> String {
    let h = ms / 3_600_000;
    let m = (ms / 60_000) % 60;
    let s = (ms / 1000) % 60;
    let msec = ms % 1000;
    format!("{h:02}:{m:02}:{s:02}.{msec:03}")
}

/// SRT numbering is 1-based; cue ids are 0-based.
pub fn to_srt(track: &Track, layout: Layout) -> String {
    let mut out = String::new();
    for cue in &track.cues {
        out.push_str(&format!(
            "{}\n{} --> {}\n",
            cue.id + 1,
            srt_time(cue.start_ms),
            srt_time(cue.end_ms)
        ));
        for line in break_lines(&cue.text, layout) {
            out.push_str(&line);
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

pub fn to_vtt(track: &Track, layout: Layout) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in &track.cues {
        out.push_str(&format!(
            "{}\n{} --> {}\n",
            cue.id + 1,
            vtt_time(cue.start_ms),
            vtt_time(cue.end_ms)
        ));
        for line in break_lines(&cue.text, layout) {
            out.push_str(&line);
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

pub fn write_srt(track: &Track, layout: Layout, path: &Path) -> std::io::Result<()> {
    std::fs::write(path, to_srt(track, layout))
}

/// `as_str` is the file extension; the UI's format list comes from `ALL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Srt,
    Txt,
    Vtt,
}

impl Format {
    pub const ALL: &'static [Format] = &[Format::Srt, Format::Txt, Format::Vtt];

    pub fn as_str(self) -> &'static str {
        match self {
            Format::Srt => "srt",
            Format::Txt => "txt",
            Format::Vtt => "vtt",
        }
    }

    pub fn parse(s: &str) -> Option<Format> {
        Format::ALL.iter().copied().find(|f| f.as_str() == s)
    }

    pub fn render(self, track: &Track, layout: Layout) -> String {
        match self {
            Format::Srt => to_srt(track, layout),
            Format::Txt => to_txt(track, layout),
            Format::Vtt => to_vtt(track, layout),
        }
    }
}

/// Lines only, no timecodes.
pub fn to_txt(track: &Track, layout: Layout) -> String {
    let mut out = String::new();
    for cue in &track.cues {
        for line in break_lines(&cue.text, layout) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

pub fn write_format(
    track: &Track,
    layout: Layout,
    format: Format,
    path: &Path,
) -> std::io::Result<()> {
    std::fs::write(path, format.render(track, layout))
}

/// Unknown extensions fall back to SRT.
pub fn format_of_path(path: &Path) -> Format {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(Format::parse)
        .unwrap_or(Format::Srt)
}

/// Refuses to write a translation whose `(id, start_ms, end_ms)` sequence
/// differs from the source.
pub fn write_srt_checked(
    source: &Track,
    translated: &Track,
    layout: Layout,
    path: &Path,
) -> Result<(), ExportError> {
    subtitler_core::assert_timecodes_preserved(source, translated)?;
    std::fs::write(path, to_srt(translated, layout))?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("timecodes are not identical to the source track: {0}")]
    TimecodesChanged(#[from] TimecodeMismatch),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtitler_core::model::{Cue, Track};

    fn track() -> Track {
        Track {
            lang: "tr".into(),
            cues: vec![
                Cue::new(0, 0, 2500, "Merhaba dünya.").unwrap(),
                Cue::new(
                    1,
                    3000,
                    6000,
                    "Bu cue iki satıra bölünmeye uygun uzunlukta bir metin barındırıyor",
                )
                .unwrap(),
            ],
            flags: vec![],
        }
    }

    #[test]
    fn srt_format_is_canonical() {
        let srt = to_srt(
            &track(),
            Layout {
                max_chars_per_line: 42,
                max_lines: 2,
            },
        );
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:02,500\nMerhaba dünya.\n\n"));
        assert!(srt.contains("2\n00:00:03,000 --> 00:00:06,000\n"));
    }

    #[test]
    fn vtt_uses_dot_milliseconds() {
        let vtt = to_vtt(
            &track(),
            Layout {
                max_chars_per_line: 42,
                max_lines: 2,
            },
        );
        assert!(vtt.starts_with("WEBVTT\n\n"));
        assert!(vtt.contains("00:00:00.000 --> 00:00:02.500"));
    }

    #[test]
    fn short_text_stays_one_line() {
        let lines = break_lines(
            "Merhaba dünya.",
            Layout {
                max_chars_per_line: 42,
                max_lines: 2,
            },
        );
        assert_eq!(lines, vec!["Merhaba dünya."]);
    }

    #[test]
    fn two_line_split_is_balanced_and_pyramid() {
        let text = "Bu cue iki satıra bölünmeye uygun uzunlukta bir metin barındırıyor";
        let lines = break_lines(
            text,
            Layout {
                max_chars_per_line: 42,
                max_lines: 2,
            },
        );
        assert_eq!(lines.len(), 2);
        assert!(
            lines.iter().all(|l| l.graphemes(true).count() <= 42),
            "{lines:?}"
        );
        assert_eq!(lines.join(" "), text);
        let (a, b) = (
            lines[0].graphemes(true).count(),
            lines[1].graphemes(true).count(),
        );
        assert!(b >= a, "pyramid: second line should be longer: {lines:?}");
    }

    #[test]
    fn write_srt_checked_rejects_timecode_drift() {
        let dir = tempfile_fix();
        let path = dir.join("out.srt");
        let mut translated = track();
        translated.lang = "de".into();
        assert!(write_srt_checked(
            &track(),
            &translated,
            Layout {
                max_chars_per_line: 42,
                max_lines: 2
            },
            &path,
        )
        .is_ok());

        translated.cues[0].start_ms = 999;
        let err = write_srt_checked(
            &track(),
            &translated,
            Layout {
                max_chars_per_line: 42,
                max_lines: 2,
            },
            &path,
        )
        .unwrap_err();
        assert!(matches!(err, ExportError::TimecodesChanged(_)));
    }

    // Avoids pulling in `tempfile` just for tests.
    fn tempfile_fix() -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("subtitler-export-test-{}", std::process::id()));
        let unique = dir.join(format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&unique).unwrap();
        unique
    }

    #[test]
    fn every_format_carries_the_same_lines() {
        let t = track();
        let layout = Layout {
            max_chars_per_line: 42,
            max_lines: 2,
        };
        // Strip headers, cue numbers and timecodes, leaving only the text lines.
        let lines_of = |s: &str| -> Vec<String> {
            s.lines()
                .filter(|l| {
                    let t = l.trim();
                    !t.is_empty()
                        && t != "WEBVTT"
                        && !t.contains("-->")
                        && !t.parse::<u32>().is_ok()
                })
                .map(|l| l.to_string())
                .collect()
        };
        let srt = to_srt(&t, layout);
        let vtt = to_vtt(&t, layout);
        let txt = to_txt(&t, layout);
        assert_eq!(lines_of(&srt), lines_of(&vtt), "SRT ve VTT satırları eşleşmeli");
        assert_eq!(lines_of(&srt), lines_of(&txt), "SRT ve TXT satırları eşleşmeli");
    }

    #[test]
    fn txt_has_no_timecodes_and_vtt_has_a_header() {
        let t = track();
        let layout = Layout {
            max_chars_per_line: 42,
            max_lines: 2,
        };
        let txt = to_txt(&t, layout);
        assert!(!txt.contains("-->"), "TXT zaman kodu taşımaz");
        assert!(!txt.contains("00:00:"), "TXT zaman kodu taşımaz");
        assert_eq!(
            txt.lines().filter(|l| !l.trim().is_empty()).count(),
            3,
            "TXT yalnızca metin satırları taşır (1 + 2 satırlık cue)"
        );

        let vtt = to_vtt(&t, layout);
        assert!(vtt.starts_with("WEBVTT\n"), "VTT başlığı olmalı");
        assert!(vtt.contains("00:00:00.000 --> 00:00:02.500"), "VTT noktalı olmalı");
        assert!(!vtt.contains("00:00:00,000"), "VTT virgüllü SRT kodu kullanmaz");
    }

    #[test]
    fn format_names_are_the_file_extension_and_roundtrip() {
        for f in Format::ALL {
            assert_eq!(Format::parse(f.as_str()), Some(*f));
        }
        assert_eq!(Format::parse("mp4"), None);
        assert_eq!(
            Format::Srt.as_str(),
            "srt",
            "uzantı küçük harf olmalı, dosya sistemi bunu bekler"
        );
    }

    #[test]
    fn format_is_derived_from_the_extension() {
        assert_eq!(format_of_path(Path::new("de.srt")), Format::Srt);
        assert_eq!(format_of_path(Path::new("de.txt")), Format::Txt);
        assert_eq!(format_of_path(Path::new("de.vtt")), Format::Vtt);
        assert_eq!(format_of_path(Path::new("de")), Format::Srt);
        assert_eq!(format_of_path(Path::new("de.mp4")), Format::Srt);
    }
}
