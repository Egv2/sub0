//! Side-by-side STT provider comparison on a single file.
//!
//! Audio is only chunked when it exceeds the upload limit or a provider's
//! per-request duration cap. OpenRouter silently drops word timestamps on
//! long requests, so its cap is lower.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::Serialize;

use subtitler_core::config::model_slugs;
use subtitler_core::model::Word;
use subtitler_media as media;
use subtitler_media::{ChunkPlan, MediaInfo};
use subtitler_segment::Profile;
use subtitler_stt::{
    shift_transcription_times, AudioChunk, AudioFormat, GroqProvider, OpenRouterProvider, Segment,
    SttProvider, TranscribeOptions,
};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ProviderArg {
    Groq,
    Openrouter,
    Both,
}

#[derive(Debug, Args)]
pub struct SpikeArgs {
    /// Local media file (video or audio) to run the spike on
    #[arg(long)]
    pub input: PathBuf,

    /// STT provider(s) to measure
    #[arg(long, value_enum, default_value = "both")]
    pub provider: ProviderArg,

    /// Override the default STT model for every selected provider
    #[arg(long)]
    pub stt_model: Option<String>,

    /// ISO-639-1 language hint (omit = auto-detect)
    #[arg(long)]
    pub language: Option<String>,

    /// Target chunk length in seconds, used only when chunking is needed
    #[arg(long, default_value_t = 600)]
    pub target_chunk_secs: u32,

    /// Hard cap in seconds for one chunk
    #[arg(long, default_value_t = 900)]
    pub max_chunk_secs: u32,

    /// Upload size limit in MB; larger files are chunked
    #[arg(long, default_value_t = 25)]
    pub max_flac_mb: u64,

    /// silencedetect noise floor
    #[arg(long, default_value = "-35dB")]
    pub noise_db: String,

    /// Minimum silence length in seconds to consider as a cut point
    #[arg(long, default_value_t = 0.4)]
    pub min_silence_secs: f64,

    /// Transcribe only the first N chunks (cost control)
    #[arg(long)]
    pub limit_chunks: Option<usize>,

    /// Also write an HTML preview into --out
    #[arg(long)]
    pub preview: bool,

    /// Output directory for artifacts and the summary
    #[arg(long, default_value = "docs/spike-data")]
    pub out: PathBuf,
}

struct SelectedProvider {
    name: &'static str,
    provider: Box<dyn SttProvider>,
    model: String,
    /// OpenRouter returns 200 but drops `words` on long requests, so it
    /// gets a lower cap than Groq.
    max_request_secs: u32,
}

#[derive(Debug, Serialize)]
struct ChunkRun {
    index: usize,
    start_ms: u32,
    end_ms: u32,
    bytes: usize,
    latency_ms: Option<u64>,
    words: usize,
    segments: usize,
    max_no_speech_prob: Option<f32>,
    avg_logprob: Option<f32>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SegmentRow {
    chunk: usize,
    segment: Segment,
}

#[derive(Debug, Serialize)]
struct ProviderReport {
    provider: String,
    model: String,
    mode: String,
    chunk_runs: Vec<ChunkRun>,
    words: Vec<Word>,
    segments: Vec<SegmentRow>,
    detected_languages: Vec<String>,
}

pub async fn run(args: SpikeArgs) -> Result<()> {
    if !args.input.is_file() {
        bail!("input file not found: {}", args.input.display());
    }
    let info = media::probe(&args.input).await.context("ffprobe failed")?;
    println!(
        "ffprobe: {} ms, fps {:?}, audio stream: {}",
        info.duration_ms, info.fps, info.has_audio
    );
    if !info.has_audio {
        bail!("input has no audio stream");
    }

    let providers = collect_providers(&args)?;

    let chunks_dir = args.out.join("chunks");
    tokio::fs::create_dir_all(&chunks_dir).await?;
    let full_flac = chunks_dir.join("full.flac");
    media::extract_audio(&args.input, &full_flac, media::AudioExtractFormat::Flac)
        .await
        .context("audio extraction failed")?;
    let flac_mb = tokio::fs::metadata(&full_flac).await?.len() as f64 / (1024.0 * 1024.0);
    println!("extracted audio: {flac_mb:.1} MB FLAC (16 kHz mono)");

    let mut reports = Vec::new();
    let mut tracks = Vec::new();
    for (index, sel) in providers.iter().enumerate() {
        println!(
            "==> '{}' ({}), single-request cap {} s",
            sel.name, sel.model, sel.max_request_secs
        );
        let (plans, files, note) =
            plan_for_provider(sel, &args, &info, flac_mb, &full_flac, &chunks_dir).await?;
        println!(
            "    plan: {} request(s), ~{} s audio — {note}",
            plans.len(),
            plans.iter().map(|p| p.duration_ms() as u64).sum::<u64>() / 1000
        );
        let dir = args.out.join(sel.name);
        let report = run_provider(
            sel,
            &plans,
            &files,
            args.language.as_deref(),
            note.clone(),
            &dir,
        )
        .await?;
        // Preview our own segmentation, not the provider's raw segments.
        let lang = report
            .detected_languages
            .first()
            .cloned()
            .unwrap_or_else(|| "und".into());
        let layout = subtitler_export::Layout {
            max_chars_per_line: Profile::default().max_chars_per_line as usize,
            max_lines: Profile::default().max_lines as usize,
        };
        let track = subtitler_segment::segment(&report.words, &Profile::default(), &lang, info.fps);
        let mut ptrack = crate::preview::track_from(
            index,
            sel.name,
            &sel.model,
            &note,
            &track,
            report.words.clone(),
            layout,
        );
        ptrack.runs = report
            .chunk_runs
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ptrack.langs = report.detected_languages.clone();
        println!(
            "    segmented: {} cues, {} flagged (netflix profile)",
            ptrack.cues.len(),
            ptrack.cues.iter().filter(|c| !c.flags.is_empty()).count()
        );
        tokio::fs::write(
            dir.join("cues.json"),
            serde_json::to_vec_pretty(&ptrack.cues)?,
        )
        .await?;
        reports.push(report);
        tracks.push(ptrack);
    }

    let summary = render_summary(&args, &reports);
    let summary_path = args.out.join("spike-summary.md");
    tokio::fs::write(&summary_path, &summary).await?;
    print!("\n{summary}");
    println!("summary written to {}", summary_path.display());
    if args.preview {
        let path = crate::preview::write_preview(&args.out, &args.input, &tracks)?;
        println!("preview written to {} (open in a browser)", path.display());
    }
    Ok(())
}

pub(crate) fn api_key(env: &str) -> Option<String> {
    std::env::var(env)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn collect_providers(args: &SpikeArgs) -> Result<Vec<SelectedProvider>> {
    let mut out = Vec::new();
    let mut missing = Vec::new();

    let wants = |p: ProviderArg| {
        matches!(args.provider, ProviderArg::Both)
            || std::mem::discriminant(&args.provider) == std::mem::discriminant(&p)
    };

    if wants(ProviderArg::Groq) {
        match api_key("GROQ_API_KEY") {
            Some(key) => out.push(SelectedProvider {
                name: "groq",
                model: args
                    .stt_model
                    .clone()
                    // large-v3 over turbo: turbo output looked truncated.
                    .unwrap_or_else(|| model_slugs::GROQ_STT_QUALITY.into()),
                provider: Box::new(GroqProvider::new(key)),
                max_request_secs: 600,
            }),
            None => missing.push("GROQ_API_KEY"),
        }
    }
    if wants(ProviderArg::Openrouter) {
        match api_key("OPENROUTER_API_KEY") {
            Some(key) => out.push(SelectedProvider {
                name: "openrouter",
                model: args
                    .stt_model
                    .clone()
                    .unwrap_or_else(|| model_slugs::OPENROUTER_STT_DEFAULT.into()),
                provider: Box::new(OpenRouterProvider::new(key)),
                max_request_secs: 300,
            }),
            None => missing.push("OPENROUTER_API_KEY"),
        }
    }

    if out.is_empty() {
        bail!(
            "no provider selected: missing API key(s) {}. Export them or pass --provider with a configured key.",
            missing.join(", ")
        );
    }
    if !missing.is_empty() {
        println!(
            "warning: skipping providers with missing keys: {}",
            missing.join(", ")
        );
    }
    Ok(out)
}

async fn plan_for_provider(
    sel: &SelectedProvider,
    args: &SpikeArgs,
    info: &MediaInfo,
    flac_mb: f64,
    full_flac: &Path,
    chunks_dir: &Path,
) -> Result<(Vec<ChunkPlan>, Vec<PathBuf>, String)> {
    let max_req_ms = sel.max_request_secs * 1000;
    let size_ok = flac_mb <= args.max_flac_mb as f64;
    let duration_ok = info.duration_ms <= max_req_ms;
    if size_ok && duration_ok {
        let plan = ChunkPlan {
            start_ms: 0,
            end_ms: info.duration_ms,
            cut_at_silence: false,
        };
        let note = format!(
            "single request (FLAC {flac_mb:.1} MB ≤ {} MB; {} s ≤ {} s provider cap)",
            args.max_flac_mb,
            info.duration_ms / 1000,
            sel.max_request_secs
        );
        return Ok((vec![plan], vec![full_flac.to_path_buf()], note));
    }

    let reason = match (size_ok, duration_ok) {
        (true, false) => format!(
            "duration {} s > {} s provider cap",
            info.duration_ms / 1000,
            sel.max_request_secs
        ),
        (false, true) => format!("FLAC {flac_mb:.1} MB > {} MB limit", args.max_flac_mb),
        (false, false) => format!(
            "FLAC {flac_mb:.1} MB > {} MB and duration {} s > {} s cap",
            args.max_flac_mb,
            info.duration_ms / 1000,
            sel.max_request_secs
        ),
        (true, true) => unreachable!("chunking requires a violated limit"),
    };

    let silences = media::detect_silences(
        full_flac,
        &args.noise_db,
        args.min_silence_secs,
        Some(info.duration_ms),
    )
    .await
    .context("silence detection failed")?;

    let target_ms = args.target_chunk_secs.min(sel.max_request_secs) * 1000;
    let cap_ms = args.max_chunk_secs.min(sel.max_request_secs) * 1000;
    let mut plans = media::plan_chunks(info.duration_ms, &silences, target_ms, cap_ms);
    if let Some(n) = args.limit_chunks {
        plans.truncate(n);
    }

    let provider_chunks = chunks_dir.join(sel.name);
    tokio::fs::create_dir_all(&provider_chunks).await?;
    let mut files = Vec::with_capacity(plans.len());
    for (i, plan) in plans.iter().enumerate() {
        let path = provider_chunks.join(format!("chunk_{i:04}.flac"));
        media::extract_audio_slice(
            &args.input,
            &path,
            plan.start_ms,
            plan.end_ms,
            media::AudioExtractFormat::Flac,
        )
        .await
        .with_context(|| format!("extracting chunk {i}"))?;
        files.push(path);
    }

    let note = format!(
        "chunked ({reason}); target {} s / cap {} s, noise {}, min silence {} s, {} silences{}",
        target_ms / 1000,
        cap_ms / 1000,
        args.noise_db,
        args.min_silence_secs,
        silences.len(),
        args.limit_chunks
            .map(|n| format!(", limited to first {n} chunks"))
            .unwrap_or_default(),
    );
    Ok((plans, files, note))
}

async fn run_provider(
    sel: &SelectedProvider,
    plans: &[ChunkPlan],
    chunk_files: &[PathBuf],
    language: Option<&str>,
    mode: String,
    provider_dir: &Path,
) -> Result<ProviderReport> {
    tokio::fs::create_dir_all(provider_dir).await?;
    let opts = TranscribeOptions {
        model: sel.model.clone(),
        language: language.map(str::to_string),
        temperature: 0.0,
        prompt: None,
    };

    let mut report = ProviderReport {
        provider: sel.name.to_string(),
        model: sel.model.clone(),
        mode,
        chunk_runs: Vec::with_capacity(plans.len()),
        words: Vec::new(),
        segments: Vec::new(),
        detected_languages: Vec::new(),
    };

    for (i, (plan, file)) in plans.iter().zip(chunk_files).enumerate() {
        let bytes = tokio::fs::read(file).await?;
        let chunk = AudioChunk {
            bytes,
            format: AudioFormat::Flac,
            offset_ms: plan.start_ms,
            duration_ms: plan.duration_ms(),
        };
        let mut run = ChunkRun {
            index: i,
            start_ms: plan.start_ms,
            end_ms: plan.end_ms,
            bytes: chunk.bytes.len(),
            latency_ms: None,
            words: 0,
            segments: 0,
            max_no_speech_prob: None,
            avg_logprob: None,
            error: None,
        };
        let started = Instant::now();
        match sel.provider.transcribe_chunk(&chunk, &opts).await {
            Ok(mut t) => {
                run.latency_ms = Some(started.elapsed().as_millis() as u64);
                run.words = t.words.len();
                run.segments = t.segments.len();
                run.max_no_speech_prob = t
                    .segments
                    .iter()
                    .map(|s| s.no_speech_prob)
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                if !t.segments.is_empty() {
                    run.avg_logprob = Some(
                        t.segments.iter().map(|s| s.avg_logprob).sum::<f32>()
                            / t.segments.len() as f32,
                    );
                }
                if let Some(lang) = t.language.clone() {
                    if !report.detected_languages.contains(&lang) {
                        report.detected_languages.push(lang);
                    }
                }
                shift_transcription_times(&mut t, plan.start_ms);
                report.words.extend(t.words.iter().cloned());
                for segment in t.segments {
                    report.segments.push(SegmentRow { chunk: i, segment });
                }
            }
            Err(e) => run.error = Some(e.to_string()),
        }
        println!(
            "  chunk {}/{} [{}–{} ms]: {} words, {}",
            i + 1,
            plans.len(),
            plan.start_ms,
            plan.end_ms,
            run.words,
            run.error
                .as_deref()
                .map(|e| format!("ERROR: {e}"))
                .unwrap_or_else(|| format!("ok in {} ms", run.latency_ms.unwrap_or(0)))
        );
        report.chunk_runs.push(run);
    }

    tokio::fs::write(
        provider_dir.join("words.json"),
        serde_json::to_vec_pretty(&report.words)?,
    )
    .await?;
    tokio::fs::write(
        provider_dir.join("segments.json"),
        serde_json::to_vec_pretty(&report.segments)?,
    )
    .await?;
    tokio::fs::write(
        provider_dir.join("chunk_runs.json"),
        serde_json::to_vec_pretty(&report.chunk_runs)?,
    )
    .await?;
    Ok(report)
}

fn render_summary(args: &SpikeArgs, reports: &[ProviderReport]) -> String {
    let mut md = String::new();
    md.push_str("# Spike run summary\n\n");
    md.push_str(&format!("- input: `{}`\n\n", args.input.display()));
    md.push_str("| provider | model | requests | words | words/audio-min | mean latency ms | langs | top repeated trigram | segs no_speech>0.9 |\n");
    md.push_str("|---|---|---|---|---|---|---|---|---|\n");
    for r in reports {
        let ok: Vec<&ChunkRun> = r.chunk_runs.iter().filter(|c| c.error.is_none()).collect();
        let failed = r.chunk_runs.len() - ok.len();
        let words: usize = ok.iter().map(|c| c.words).sum();
        let covered_ms: u64 = r
            .chunk_runs
            .iter()
            .map(|c| (c.end_ms - c.start_ms) as u64)
            .sum();
        let audio_min = (covered_ms as f64 / 60_000.0).max(f64::EPSILON);
        let mean_latency = if ok.is_empty() {
            0.0
        } else {
            ok.iter().filter_map(|c| c.latency_ms).sum::<u64>() as f64 / ok.len() as f64
        };
        let (trigram, trigram_n) = top_trigram(&r.words).unwrap_or(("-".into(), 0));
        let silent = r
            .segments
            .iter()
            .filter(|s| s.segment.no_speech_prob > 0.9)
            .count();
        md.push_str(&format!(
            "| {} | {} | {}/{} | {} | {:.1} | {:.0} | {} | “{}” ×{} | {} |\n",
            r.provider,
            r.model,
            ok.len(),
            failed,
            words,
            words as f64 / audio_min,
            mean_latency,
            if r.detected_languages.is_empty() {
                "-".into()
            } else {
                r.detected_languages.join(", ")
            },
            trigram,
            trigram_n,
            silent,
        ));
    }
    md.push_str("\n## Modes\n\n");
    for r in reports {
        md.push_str(&format!("- **{}** ({}): {}\n", r.provider, r.model, r.mode));
    }
    if reports
        .iter()
        .any(|r| r.chunk_runs.iter().any(|c| c.error.is_some()))
    {
        md.push_str("\n## Chunk errors\n\n");
        for r in reports {
            for c in &r.chunk_runs {
                if let Some(err) = &c.error {
                    md.push_str(&format!("- {} chunk {}: {}\n", r.provider, c.index, err));
                }
            }
        }
    }
    md
}

/// Most frequent 3-gram; a cheap signal for repetition hallucinations.
fn top_trigram(words: &[Word]) -> Option<(String, usize)> {
    use std::collections::HashMap;
    let norm = |s: &str| {
        s.trim()
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase()
    };
    if words.len() < 3 {
        return None;
    }
    let mut counts: HashMap<[String; 3], usize> = HashMap::new();
    for w in words.windows(3) {
        let key = [norm(&w[0].text), norm(&w[1].text), norm(&w[2].text)];
        if key.iter().all(String::is_empty) {
            continue;
        }
        *counts.entry(key).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(k, n)| (k.join(" "), n))
}
