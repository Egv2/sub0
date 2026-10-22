//! Silero VAD. ffmpeg streams raw f32 samples into a worker thread that owns
//! the ONNX session, so memory stays flat on multi-hour inputs.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use silero::{Session, SpeechOptions, SpeechSegmenter, SpeechSegmenterExt, StreamState};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::error::MediaError;
use crate::ffmpeg::Silence;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpeechRange {
    pub start_ms: u32,
    pub end_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadParams {
    pub threshold: f32,
    pub min_speech_ms: u32,
    pub min_silence_ms: u32,
    pub speech_pad_ms: u32,
}

impl VadParams {
    fn options(&self) -> SpeechOptions {
        SpeechOptions::default()
            .with_start_threshold(self.threshold)
            .with_min_speech_duration(Duration::from_millis(self.min_speech_ms as u64))
            .with_min_silence_duration(Duration::from_millis(self.min_silence_ms as u64))
            .with_speech_pad(Duration::from_millis(self.speech_pad_ms as u64))
    }
}

const READ_SAMPLES: usize = 16_000;

pub async fn detect_speech(audio: &Path, params: VadParams) -> Result<Vec<SpeechRange>, MediaError> {
    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(audio)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => MediaError::BinaryNotFound("ffmpeg".into()),
            _ => MediaError::Io(e),
        })?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    // Drained concurrently: a full stderr pipe would stall ffmpeg.
    let mut stderr = child.stderr.take().expect("piped stderr");
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        buf
    });

    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<f32>>(8);
    let options = params.options();
    let worker = tokio::task::spawn_blocking(move || run_vad(rx, options));

    let mut buf = vec![0u8; READ_SAMPLES * 4];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = stdout.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..n]);
        let whole = carry.len() / 4 * 4;
        let samples: Vec<f32> = carry[..whole]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        carry.drain(..whole);
        if tx.send(samples).await.is_err() {
            break;
        }
    }
    drop(tx);
    drop(stdout);

    let status = child.wait().await?;
    let stderr = stderr_task.await.unwrap_or_default();
    let ranges = worker
        .await
        .map_err(|_| MediaError::Vad("VAD worker panicked".into()))??;
    if !status.success() {
        return Err(MediaError::ToolFailed {
            binary: "ffmpeg".into(),
            code: status.code().unwrap_or(-1),
            stderr: crate::util::truncate_utf8(&stderr, 4000),
        });
    }
    Ok(ranges)
}

fn run_vad(mut rx: tokio::sync::mpsc::Receiver<Vec<f32>>, options: SpeechOptions) -> Result<Vec<SpeechRange>, MediaError> {
    let vad_err = |e: silero::Error| MediaError::Vad(e.to_string());
    let mut session = Session::bundled().map_err(vad_err)?;
    let mut stream = StreamState::new(options.sample_rate());
    let mut segmenter = SpeechSegmenter::new(options);
    let mut ranges = Vec::new();
    let mut push = |seg: silero::SpeechSegment| {
        ranges.push(SpeechRange {
            start_ms: (seg.start_seconds() * 1000.0).round() as u32,
            end_ms: (seg.end_seconds() * 1000.0).round() as u32,
        });
    };
    while let Some(samples) = rx.blocking_recv() {
        let mut next = segmenter
            .push_samples(&mut session, &mut stream, &samples)
            .map_err(vad_err)?;
        while let Some(seg) = next {
            push(seg);
            next = segmenter
                .push_samples(&mut session, &mut stream, &[])
                .map_err(vad_err)?;
        }
    }
    let mut next = segmenter
        .finish_stream(&mut session, &mut stream)
        .map_err(vad_err)?;
    while let Some(seg) = next {
        push(seg);
        next = segmenter
            .push_samples(&mut session, &mut stream, &[])
            .map_err(vad_err)?;
    }
    Ok(ranges)
}

/// Non-speech gaps between ranges, for the chunk planner.
pub fn gaps(ranges: &[SpeechRange], total_ms: u32) -> Vec<Silence> {
    let mut out = Vec::new();
    let mut cursor = 0u32;
    for r in ranges {
        if r.start_ms > cursor {
            out.push(Silence {
                start_ms: cursor,
                end_ms: r.start_ms,
            });
        }
        cursor = cursor.max(r.end_ms);
    }
    if total_ms > cursor {
        out.push(Silence {
            start_ms: cursor,
            end_ms: total_ms,
        });
    }
    out
}

/// Whether `[start_ms, end_ms)` overlaps any range widened by `tolerance_ms`.
pub fn overlaps_speech(ranges: &[SpeechRange], start_ms: u32, end_ms: u32, tolerance_ms: u32) -> bool {
    let idx = ranges.partition_point(|r| r.end_ms.saturating_add(tolerance_ms) <= start_ms);
    ranges
        .get(idx)
        .is_some_and(|r| r.start_ms.saturating_sub(tolerance_ms) < end_ms.max(start_ms + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(start_ms: u32, end_ms: u32) -> SpeechRange {
        SpeechRange { start_ms, end_ms }
    }

    #[test]
    fn gaps_cover_complement() {
        let g = gaps(&[r(1000, 2000), r(3000, 4000)], 5000);
        assert_eq!(
            g,
            vec![
                Silence { start_ms: 0, end_ms: 1000 },
                Silence { start_ms: 2000, end_ms: 3000 },
                Silence { start_ms: 4000, end_ms: 5000 },
            ]
        );
    }

    #[test]
    fn overlap_respects_tolerance() {
        let ranges = [r(1000, 2000), r(5000, 6000)];
        assert!(overlaps_speech(&ranges, 1500, 1600, 0));
        assert!(overlaps_speech(&ranges, 2100, 2300, 200));
        assert!(!overlaps_speech(&ranges, 2300, 2500, 200));
        assert!(!overlaps_speech(&ranges, 3000, 4000, 200));
        assert!(overlaps_speech(&ranges, 4900, 4950, 100));
        assert!(!overlaps_speech(&[], 0, 10, 1000));
    }

    /// `SUBTITLER_VAD_SAMPLE=<audio> cargo test -p subtitler-media vad_smoke -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn vad_smoke() {
        let path = std::env::var("SUBTITLER_VAD_SAMPLE").expect("SUBTITLER_VAD_SAMPLE");
        let ranges = detect_speech(
            Path::new(&path),
            VadParams {
                threshold: 0.5,
                min_speech_ms: 250,
                min_silence_ms: 300,
                speech_pad_ms: 200,
            },
        )
        .await
        .unwrap();
        println!("{ranges:?}");
        assert!(!ranges.is_empty());
    }
}
