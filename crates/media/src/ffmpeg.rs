use std::path::Path;

use tokio::process::Command;

use subtitler_core::time::secs_to_ms;

use crate::error::MediaError;
use crate::util::run_tool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Silence {
    pub start_ms: u32,
    pub end_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioExtractFormat {
    Flac,
    WavPcm16,
}

impl AudioExtractFormat {
    fn codec_args(self) -> &'static [&'static str] {
        match self {
            AudioExtractFormat::Flac => &["-c:a", "flac"],
            AudioExtractFormat::WavPcm16 => &["-c:a", "pcm_s16le"],
        }
    }
}

pub async fn extract_audio(
    input: &Path,
    output: &Path,
    format: AudioExtractFormat,
) -> Result<(), MediaError> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(input)
        .args(["-vn", "-ac", "1", "-ar", "16000"])
        .args(format.codec_args())
        .arg(output);
    run_tool("ffmpeg", cmd).await?;
    Ok(())
}

/// Input-side `-ss` is sample-accurate for audio and stays fast on long files.
pub async fn extract_audio_slice(
    input: &Path,
    output: &Path,
    start_ms: u32,
    end_ms: u32,
    format: AudioExtractFormat,
) -> Result<(), MediaError> {
    if start_ms >= end_ms {
        return Err(MediaError::InvalidTimeRange { start_ms, end_ms });
    }
    let start_s = start_ms as f64 / 1000.0;
    let duration_s = (end_ms - start_ms) as f64 / 1000.0;
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-hide_banner", "-loglevel", "error"])
        .arg("-ss")
        .arg(format!("{start_s:.3}"))
        .arg("-i")
        .arg(input)
        .arg("-t")
        .arg(format!("{duration_s:.3}"))
        .args(["-vn", "-ac", "1", "-ar", "16000"])
        .args(format.codec_args())
        .arg(output);
    run_tool("ffmpeg", cmd).await?;
    Ok(())
}

/// `total_ms` closes a silence still open at EOF.
pub async fn detect_silences(
    input: &Path,
    noise_db: &str,
    min_silence_secs: f64,
    total_ms: Option<u32>,
) -> Result<Vec<Silence>, MediaError> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-nostats", "-i"])
        .arg(input)
        .arg("-af")
        .arg(format!(
            "silencedetect=noise={noise_db}:d={min_silence_secs}"
        ))
        .args(["-f", "null", "-"]);
    let out = run_tool("ffmpeg", cmd).await?;
    Ok(parse_silencedetect(
        &String::from_utf8_lossy(&out.stderr),
        total_ms,
    ))
}

/// Lines look like `silence_start: 12.345` and
/// `silence_end: 13.456 | silence_duration: 1.111`.
fn parse_silencedetect(stderr: &str, total_ms: Option<u32>) -> Vec<Silence> {
    let mut out = Vec::new();
    let mut open: Option<u32> = None;
    for line in stderr.lines() {
        if let Some(rest) = line.split("silence_start: ").nth(1) {
            if let Ok(v) = rest.trim().parse::<f64>() {
                open = Some(secs_to_ms(v));
            }
        } else if let Some(rest) = line.split("silence_end: ").nth(1) {
            if let Some(end) = rest
                .split('|')
                .next()
                .and_then(|t| t.trim().parse::<f64>().ok())
            {
                if let Some(start) = open.take() {
                    out.push(Silence {
                        start_ms: start,
                        end_ms: secs_to_ms(end),
                    });
                }
            }
        }
    }
    if let Some(start) = open {
        out.push(Silence {
            start_ms: start,
            end_ms: total_ms.unwrap_or(start),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_silencedetect_output() {
        let stderr = "\
[silencedetect @ 0x0] silence_start: 1.234
[silencedetect @ 0x0] silence_end: 2.5 | silence_duration: 1.266
[silencedetect @ 0x0] silence_start: 9
";
        let silences = parse_silencedetect(stderr, Some(12_000));
        assert_eq!(
            silences,
            vec![
                Silence {
                    start_ms: 1234,
                    end_ms: 2500
                },
                Silence {
                    start_ms: 9000,
                    end_ms: 12_000
                },
            ]
        );
    }

    #[test]
    fn ignores_noise_lines() {
        let silences = parse_silencedetect("Input #0, flac, from 'x':\nStream mapping:\n", None);
        assert!(silences.is_empty());
    }
}
