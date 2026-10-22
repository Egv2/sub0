use std::path::Path;

use serde::Deserialize;
use tokio::process::Command;

use subtitler_core::time::secs_to_ms;

use crate::error::MediaError;
use crate::util::run_tool;

#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    pub duration_ms: u32,
    /// `None` for audio-only inputs.
    pub fps: Option<f64>,
    pub has_audio: bool,
}

pub async fn probe(path: &Path) -> Result<MediaInfo, MediaError> {
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(path);
    let out = run_tool("ffprobe", cmd).await?;

    let parsed: FfprobeJson = serde_json::from_slice(&out.stdout)
        .map_err(|e| MediaError::Parse("ffprobe".into(), e.to_string()))?;

    let duration_ms = parsed
        .format
        .duration
        .as_deref()
        .and_then(|d| d.parse::<f64>().ok())
        .map(secs_to_ms)
        .ok_or_else(|| MediaError::Parse("ffprobe".into(), "missing format.duration".into()))?;

    let fps = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"))
        .and_then(|s| {
            let rate = s.r_frame_rate.as_deref()?;
            let (num, den) = rate.split_once('/')?;
            let (num, den): (f64, f64) = (num.trim().parse().ok()?, den.trim().parse().ok()?);
            (den != 0.0).then(|| num / den)
        });

    let has_audio = parsed
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("audio"));

    Ok(MediaInfo {
        duration_ms,
        fps,
        has_audio,
    })
}

#[derive(Debug, Deserialize, Default)]
struct FfprobeJson {
    #[serde(default)]
    format: FfFormat,
    #[serde(default)]
    streams: Vec<FfStream>,
}

#[derive(Debug, Deserialize, Default)]
struct FfFormat {
    #[serde(default)]
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfStream {
    #[serde(default)]
    codec_type: Option<String>,
    #[serde(default)]
    r_frame_rate: Option<String>,
}
