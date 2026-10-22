//! `yt-dlp` wrapper: downloads, playlist expansion and failure classification.

use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::error::MediaError;
use crate::util::run_tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// Retry with backoff (network hiccups, 429, 5xx, timeouts).
    Transient,
    /// Private, removed, geo-blocked, unsupported, login required.
    Permanent,
}

pub fn failure_class(err: &MediaError) -> FailureClass {
    match err {
        MediaError::ToolFailed { stderr, .. } => classify_failure(stderr),
        MediaError::BinaryNotFound(_) | MediaError::Vad(_) => FailureClass::Permanent,
        _ => FailureClass::Transient,
    }
}

pub fn classify_failure(stderr: &str) -> FailureClass {
    let lower = stderr.to_lowercase();
    let permanent_markers = [
        "private video",
        "video unavailable",
        "is unavailable",
        "removed by the uploader",
        "geo-restricted",
        "not available in your country",
        "unsupported url",
        "is not a valid url",
        "login required",
        "sign in to confirm",
        "available to members only",
        "members-only",
        "premieres in",
    ];
    if permanent_markers.iter().any(|m| lower.contains(m)) {
        return FailureClass::Permanent;
    }
    let transient_markers = [
        "http error 429",
        "http error 5",
        "temporary failure",
        "connection reset",
        "connection timed out",
        "timed out",
        "unable to download",
        "network is unreachable",
        "getaddrinfo",
    ];
    if transient_markers.iter().any(|m| lower.contains(m)) {
        return FailureClass::Transient;
    }
    // Unknown: retry; the queue caps attempts anyway.
    FailureClass::Transient
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadedVideo {
    pub path: PathBuf,
    pub final_percent: Option<u8>,
}

/// Doubles as the "is yt-dlp on PATH" check.
pub async fn version() -> Result<String, MediaError> {
    let mut cmd = Command::new("yt-dlp");
    cmd.arg("--version");
    let out = run_tool("yt-dlp", cmd).await?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The output path comes from `--print after_move:filepath`, not from guessing
/// yt-dlp's naming.
pub async fn download(
    url: &str,
    out_dir: &Path,
    audio_only: bool,
) -> Result<DownloadedVideo, MediaError> {
    tokio::fs::create_dir_all(out_dir).await?;
    let format = if audio_only {
        "bestaudio/best"
    } else {
        "bestvideo*+bestaudio/best"
    };
    let mut cmd = Command::new("yt-dlp");
    cmd.args([
        "--no-playlist",
        "--no-warnings",
        "-f",
        format,
        "--no-simulate",
        "--print",
        "after_move:filepath",
        "--newline",
        "--progress-template",
        "download:PROGRESS:%(progress._percent_str)s",
        "-P",
    ])
    .arg(out_dir)
    .arg("-o")
    .arg("source.%(ext)s")
    .arg(url);
    let out = run_tool("yt-dlp", cmd).await?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut path: Option<PathBuf> = None;
    let mut final_percent = None;
    for line in stdout.lines() {
        if let Some(percent) = parse_progress(line) {
            final_percent = Some(percent);
        } else {
            let candidate_text = line.strip_prefix("[download] ").unwrap_or(line).trim();
            if candidate_text.is_empty() {
                continue;
            }
            let candidate = PathBuf::from(candidate_text);
            if candidate.is_file() {
                path = Some(candidate);
            }
        }
    }
    match path {
        Some(path) => Ok(DownloadedVideo {
            path,
            final_percent,
        }),
        None => Err(MediaError::Parse(
            "yt-dlp".into(),
            "no after_move:filepath in output".into(),
        )),
    }
}

/// A single video expands to just its own URL, so callers can treat every input alike.
pub async fn expand_playlist(url: &str) -> Result<Vec<String>, MediaError> {
    let mut cmd = Command::new("yt-dlp");
    cmd.args(["--flat-playlist", "--no-warnings", "--print", "url"])
        .arg(url);
    let out = run_tool("yt-dlp", cmd).await?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let urls = parse_flat_playlist(&stdout);
    if urls.is_empty() {
        return Err(MediaError::Parse(
            "yt-dlp".into(),
            "playlist expansion produced no entries".into(),
        ));
    }
    Ok(urls)
}

/// One URL per line; blanks skipped, duplicates dropped (first one wins).
pub fn parse_flat_playlist(stdout: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut urls = Vec::new();
    for line in stdout.lines() {
        let url = line.trim();
        if url.is_empty() || !seen.insert(url) {
            continue;
        }
        urls.push(url.to_string());
    }
    urls
}

/// Parses our `--progress-template` lines, e.g. `PROGRESS: 42.0%`.
pub fn parse_progress(line: &str) -> Option<u8> {
    let rest = line.strip_prefix("PROGRESS:")?.trim();
    let percent = rest.strip_suffix('%')?.trim().parse::<f32>().ok()?;
    if !(0.0..=100.0).contains(&percent) {
        return None;
    }
    Some(percent.round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_permanent_failures() {
        for stderr in [
            "ERROR: [youtube] xyz: Private video. Sign in if you've been granted access",
            "ERROR: This video is unavailable",
            "ERROR: Unsupported URL: https://example.invalid/x",
            "ERROR: [youtube] xyz: Sign in to confirm you're not a bot",
            "ERROR: This video is available to members only",
        ] {
            assert_eq!(
                classify_failure(stderr),
                FailureClass::Permanent,
                "{stderr}"
            );
        }
    }

    #[test]
    fn classifies_transient_failures() {
        for stderr in [
            "ERROR: unable to download video data: HTTP Error 429: Too Many Requests",
            "ERROR: unable to download video data: HTTP Error 503: Service Unavailable",
            "ERROR: Connection reset by peer",
            "WARNING: ... unable to download webpage: <urlopen error timed out>",
        ] {
            assert_eq!(
                classify_failure(stderr),
                FailureClass::Transient,
                "{stderr}"
            );
        }
    }

    #[test]
    fn unknown_failures_default_to_transient() {
        assert_eq!(
            classify_failure("ERROR: something novel happened"),
            FailureClass::Transient
        );
    }

    #[test]
    fn parses_progress_lines() {
        assert_eq!(parse_progress("PROGRESS: 42.0%"), Some(42));
        assert_eq!(parse_progress("PROGRESS:100.0%"), Some(100));
        assert_eq!(parse_progress("PROGRESS: 101.0%"), None);
        assert_eq!(parse_progress("[download] 42.0% of 1.0M"), None);
        assert_eq!(parse_progress(""), None);
    }

    #[test]
    fn parses_flat_playlist_output() {
        let out = "https://e.test/v1\n\n  https://e.test/v2  \nhttps://e.test/v3\n";
        assert_eq!(
            parse_flat_playlist(out),
            vec![
                "https://e.test/v1".to_string(),
                "https://e.test/v2".to_string(),
                "https://e.test/v3".to_string(),
            ]
        );
    }

    #[test]
    fn single_video_prints_itself_and_duplicates_are_dropped() {
        assert_eq!(
            parse_flat_playlist("https://e.test/v1\n"),
            vec!["https://e.test/v1"]
        );
        let dupes = "https://e.test/v1\nhttps://e.test/v2\nhttps://e.test/v1\n";
        assert_eq!(
            parse_flat_playlist(dupes),
            vec!["https://e.test/v1", "https://e.test/v2"]
        );
        assert!(parse_flat_playlist("\n   \n").is_empty());
    }
}
