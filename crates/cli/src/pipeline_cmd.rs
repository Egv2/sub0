//! Thin CLI over `subtitler-pipeline`; no pipeline logic lives here.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;
use subtitler_core::lang;
use subtitler_core::config::{llm_presets, model_slugs, LlmSettings, ProviderKind, SttSettings};
use subtitler_core::model::CueFlag;
use subtitler_export::Layout;
use subtitler_segment::Profile;
use subtitler_stt::{GroqProvider, OpenRouterProvider, SttProvider};
use subtitler_translate::OpenAiCompatLlm;

use crate::{preview, spike::api_key};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ModeArg {
    Fast,
    Pro,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ProviderArg {
    Groq,
    Openrouter,
}

impl ProviderArg {
    fn kind(self) -> ProviderKind {
        match self {
            ProviderArg::Groq => ProviderKind::Groq,
            ProviderArg::Openrouter => ProviderKind::OpenRouter,
        }
    }

    fn env_key(self) -> &'static str {
        match self {
            ProviderArg::Groq => "GROQ_API_KEY",
            ProviderArg::Openrouter => "OPENROUTER_API_KEY",
        }
    }

    fn name(self) -> &'static str {
        match self {
            ProviderArg::Groq => "groq",
            ProviderArg::Openrouter => "openrouter",
        }
    }

    /// OpenRouter silently drops word timestamps on long requests, so its cap is lower.
    fn max_request_secs(self) -> u32 {
        match self {
            ProviderArg::Groq => 600,
            ProviderArg::Openrouter => 300,
        }
    }
}

#[derive(Debug, Args)]
pub struct SharedArgs {
    /// Local media file (video or audio) — not used by `batch`
    #[arg(long)]
    pub input: Option<PathBuf>,

    /// STT provider (selectable per job — Groq or OpenRouter)
    #[arg(long, value_enum, default_value = "groq")]
    pub provider: ProviderArg,

    /// Override the provider's default STT model slug
    #[arg(long)]
    pub stt_model: Option<String>,

    /// ISO-639-1 source language hint (omit = auto-detect)
    #[arg(long)]
    pub language: Option<String>,

    /// Subtitle profile TOML (defaults to the built-in Netflix profile)
    #[arg(long)]
    pub profile: Option<PathBuf>,

    /// Also render an interactive HTML preview (video + every track) into --out
    #[arg(long)]
    pub preview: bool,

    /// Run the optional cleanup stage (LLM word-index deletion) before
    /// segmentation; needs an LLM provider (the translation provider is reused)
    #[arg(long)]
    pub cleanup: bool,

    /// Explicit cleanup prompt file (default: prompts/cleanup.{lang}.md,
    /// falling back to cleanup.en.md)
    #[arg(long)]
    pub cleanup_prompt: Option<PathBuf>,

    /// Max share of words the LLM may delete per window; above it only
    /// non-lexical deletions (captions, fillers, symbols) are kept
    #[arg(long, default_value_t = 0.2)]
    pub cleanup_ratio: f32,

    /// Translation quality: `fast` (batched, neighbour context) or `pro`
    /// (localization brief + style profile + translated context)
    #[arg(long, value_enum, default_value_t = ModeArg::Pro)]
    pub mode: ModeArg,

    /// Skip the deterministic caption/filler/loop removal
    #[arg(long)]
    pub no_rules: bool,

    /// Skip Silero VAD (fall back to ffmpeg silence detection, no word filter)
    #[arg(long)]
    pub no_vad: bool,

    /// Chunking is conditional: only above this FLAC size (MB)
    #[arg(long, default_value_t = 25)]
    pub max_flac_mb: u64,

    /// Chunk target seconds (only when chunking happens)
    #[arg(long, default_value_t = 600)]
    pub target_chunk_secs: u32,

    /// Chunk hard cap seconds
    #[arg(long, default_value_t = 900)]
    pub max_chunk_secs: u32,

    /// silencedetect noise floor
    #[arg(long, default_value = "-35dB")]
    pub noise_db: String,

    /// Minimum silence seconds to cut at
    #[arg(long, default_value_t = 0.4)]
    pub min_silence_secs: f64,

    /// Output directory for SRTs, artifacts and the preview
    #[arg(long, default_value = "data/run")]
    pub out: PathBuf,

    /// Disable the persistent LLM/STT cache (data/store.db)
    #[arg(long)]
    pub no_cache: bool,

    /// Drop words in the first N seconds (e.g. a music intro that triggers
    /// Whisper hallucinations)
    #[arg(long, default_value_t = 0)]
    pub trim_start_secs: u32,

    /// Drop words in the last N seconds (e.g. an outro jingle)
    #[arg(long, default_value_t = 0)]
    pub trim_end_secs: u32,
}

#[derive(Debug, Args)]
pub struct TranscribeArgs {
    #[command(flatten)]
    pub shared: SharedArgs,
}

/// Translation/cleanup settings shared by `run` and `batch`.
#[derive(Debug, Args)]
pub struct LlmArgs {
    /// Comma-separated target languages, e.g. `de,en`
    #[arg(long, value_delimiter = ',')]
    pub langs: Vec<String>,

    /// LLM provider for translation/cleanup (same platforms offer LLMs + STT)
    #[arg(long, value_enum, default_value = "groq")]
    pub llm_provider: ProviderArg,

    /// Curated cheap OpenRouter preset (see `subtitler llm-presets`)
    #[arg(long)]
    pub llm_preset: Option<String>,

    /// LLM model slug for translation/cleanup
    #[arg(long)]
    pub llm_model: Option<String>,

    /// Style directives appended to the translation system prompt
    #[arg(long, default_value = "")]
    pub prompt: String,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[command(flatten)]
    pub shared: SharedArgs,

    #[command(flatten)]
    pub llm: LlmArgs,
}

#[derive(Debug, Args)]
pub struct BatchArgs {
    #[command(flatten)]
    pub shared: SharedArgs,

    #[command(flatten)]
    pub llm: LlmArgs,

    /// Local paths and/or URLs (files and links can be mixed)
    #[arg(long, value_delimiter = ',')]
    pub inputs: Vec<String>,

    /// Continue an existing batch instead of creating a new one
    #[arg(long)]
    pub resume: Option<i64>,

    /// Requeue this batch's failed jobs before draining (requires --resume)
    #[arg(long, requires = "resume")]
    pub retry_failed: bool,

    /// Upcoming URL jobs to prefetch downloads for while the current job
    /// processes (0 disables prefetching)
    #[arg(long, default_value_t = 2)]
    pub prefetch_ahead: u32,

    /// Process at most N queued jobs in this invocation
    #[arg(long)]
    pub limit: Option<u64>,
}

pub async fn transcribe(args: TranscribeArgs) -> Result<()> {
    let shared = &args.shared;
    let input = require_input(shared)?;
    let (stt, stt_name, cfg) = build(shared, Vec::new(), String::new())?;
    flow(FlowRun {
        input,
        shared,
        stt,
        stt_name,
        llm: None,
        llm_name: None,
        cfg,
        label: "transcribe",
    })
    .await
}

fn require_input(shared: &SharedArgs) -> Result<&Path> {
    let input = shared.input.as_deref().context("--input is required")?;
    if !input.is_file() {
        bail!("input file not found: {}", input.display());
    }
    Ok(input)
}

// Unlike the settings file (repaired on load), a typo'd flag must fail loudly.
// Codes also become `{target}.srt` filenames, so they can't go through unchecked.
fn canonical_langs(langs: &[String]) -> Result<Vec<String>> {
    langs
        .iter()
        .map(|code| match lang::by_code(code) {
            Some(lang) => Ok(lang.code.to_string()),
            None => bail!(
                "unknown target language {code:?}: expected an ISO-639-1 code \
                 such as `de`, `tr`, `zh-Hans`"
            ),
        })
        .collect()
}

pub async fn run(args: RunArgs) -> Result<()> {
    let shared = &args.shared;
    let input = require_input(shared)?;
    if args.llm.langs.is_empty() {
        bail!(
            "--langs is required for `run` (e.g. --langs de,en); use `transcribe` for source-only"
        );
    }
    let langs = canonical_langs(&args.llm.langs)?;
    let (kind, llm_model) = resolve_llm_model(&args.llm)?;
    let (stt, stt_name, mut cfg) = build(shared, langs, args.llm.prompt.clone())?;
    cfg.llm = LlmSettings {
        provider: kind,
        model: llm_model,
        temperature: 0.0,
    };
    let (llm, llm_name) = if cfg.langs.is_empty() && cfg.cleanup.is_none() {
        (None, None)
    } else {
        let (llm, name, _) = build_llm(&args.llm)?;
        (Some(llm), Some(name))
    };
    flow(FlowRun {
        input,
        shared,
        stt,
        stt_name,
        llm,
        llm_name,
        cfg,
        label: "run",
    })
    .await
}

fn resolve_llm_model(args: &LlmArgs) -> Result<(ProviderKind, String)> {
    if args.llm_preset.is_some() && args.llm_model.is_some() {
        bail!("--llm-preset and --llm-model are mutually exclusive");
    }
    let model = match (&args.llm_preset, &args.llm_model) {
        (Some(preset), None) => llm_presets::resolve_openrouter(preset)
            .map(str::to_string)
            .ok_or_else(|| {
                let names: Vec<&str> = llm_presets::OPENROUTER.iter().map(|(n, _)| *n).collect();
                anyhow::anyhow!(
                    "unknown preset `{preset}` — available: {}",
                    names.join(", ")
                )
            })?,
        (None, Some(slug)) => slug.clone(),
        (None, None) => model_slugs::GROQ_LLM_TRANSLATE.into(),
        (Some(_), Some(_)) => unreachable!("checked above"),
    };
    Ok((args.llm_provider.kind(), model))
}

fn build_llm(
    args: &LlmArgs,
) -> Result<(Box<dyn subtitler_translate::LlmProvider>, String, String)> {
    let llm_model: String = match (&args.llm_preset, &args.llm_model) {
        (Some(preset), None) => llm_presets::resolve_openrouter(preset)
            .map(str::to_string)
            .ok_or_else(|| {
                let names: Vec<&str> = llm_presets::OPENROUTER.iter().map(|(n, _)| *n).collect();
                anyhow::anyhow!(
                    "unknown preset `{preset}` — available: {}",
                    names.join(", ")
                )
            })?,
        (None, Some(slug)) => slug.clone(),
        (None, None) => model_slugs::GROQ_LLM_TRANSLATE.into(),
        (Some(_), Some(_)) => unreachable!("checked above"),
    };
    let key = api_key(args.llm_provider.env_key())
        .with_context(|| format!("missing {}", args.llm_provider.env_key()))?;
    let llm: Box<dyn subtitler_translate::LlmProvider> = match args.llm_provider {
        ProviderArg::Groq => Box::new(OpenAiCompatLlm::groq(key)),
        ProviderArg::Openrouter => Box::new(OpenAiCompatLlm::openrouter(key)),
    };
    let name = llm.name().to_string();
    Ok((llm, name, llm_model))
}

/// Enqueue inputs and drain the persistent queue. Re-run with `--resume <id>`
/// to continue a crashed or cancelled batch.
pub async fn batch(args: BatchArgs) -> Result<()> {
    let shared = &args.shared;
    if args.inputs.is_empty() && args.resume.is_none() {
        bail!("--inputs is required (or --resume <batch_id> to continue an earlier batch)");
    }
    let langs = canonical_langs(&args.llm.langs)?;
    let (kind, llm_model) = resolve_llm_model(&args.llm)?;
    let needs_llm = !langs.is_empty() || args.shared.cleanup;
    let (stt, stt_name, mut cfg) = build(shared, langs, args.llm.prompt.clone())?;
    cfg.llm = LlmSettings {
        provider: kind,
        model: llm_model,
        temperature: 0.0,
    };
    let (llm, llm_name) = if needs_llm {
        let (llm, name, _) = build_llm(&args.llm)?;
        (Some(llm), Some(name))
    } else {
        (None, None)
    };

    let cache_path = cfg.cache_path.clone().context(
        "the queue requires the persistent store (--no-cache cannot be used with batch)",
    )?;
    let store = std::sync::Arc::new(subtitler_store::Store::open(&cache_path).await?);

    let batch_id = match args.resume {
        Some(id) => id,
        None => {
            let settings = serde_json::to_string(&cfg)?;
            let batch_id = store.create_batch(&settings).await?;
            let mut enqueued = 0usize;
            for input in args.inputs.iter() {
                let kind = if input.starts_with("http://") || input.starts_with("https://") {
                    "url"
                } else {
                    "file"
                };
                if kind == "file" && !Path::new(input).is_file() {
                    bail!("input file not found: {input}");
                }
                // A single video "expands" to itself.
                let entries: Vec<String> = if kind == "url" {
                    subtitler_media::expand_playlist(input)
                        .await
                        .with_context(|| format!("expanding {input}"))?
                } else {
                    vec![input.clone()]
                };
                for entry in &entries {
                    // Same naming function as the app, so CLI and GUI folders match.
                    let job_dir = subtitler_pipeline::naming::job_dir(
                        &shared.out,
                        entry.as_str(),
                    )?;
                    store.enqueue_job(batch_id, "url", entry, &job_dir).await?;
                    enqueued += 1;
                }
                if entries.len() > 1 {
                    println!("playlist {input}: {} entries", entries.len());
                }
            }
            println!("batch {batch_id}: {enqueued} job(s) enqueued");
            batch_id
        }
    };

    if args.retry_failed {
        let requeued = store.requeue_failed(Some(batch_id)).await?;
        println!("batch {batch_id}: requeued {requeued} failed job(s)");
    }

    let (events, mut rx) = tokio::sync::broadcast::channel::<subtitler_core::job::JobEvent>(256);
    let printer = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            println!("  \u{2699} {event:?}");
        }
    });
    let cancel = tokio_util::sync::CancellationToken::new();
    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                println!("\ncancel requested — rerun the same command (with --resume) to continue");
                cancel.cancel();
            }
        });
    }

    println!(
        "[batch {batch_id}] stt: {} / {} · llm: {} · langs: {}",
        stt_name,
        cfg.stt.model,
        llm_name.as_deref().unwrap_or("-"),
        if cfg.langs.is_empty() {
            "-".into()
        } else {
            cfg.langs.join(",")
        }
    );
    let summary = subtitler_pipeline::queue::run_queue(subtitler_pipeline::queue::QueueRun {
        store,
        stt,
        llm,
        cfg,
        events,
        batch: Some(batch_id),
        limit: args.limit,
        prefetch_ahead: args.prefetch_ahead,
        cancel,
    })
    .await?;
    printer.abort();
    println!(
        "batch {batch_id}: {} done, {} failed{}",
        summary.done,
        summary.failed,
        if summary.cancelled {
            " (cancelled — rerun with --resume to continue)"
        } else {
            ""
        }
    );
    Ok(())
}

fn build(
    shared: &SharedArgs,
    langs: Vec<String>,
    prompt: String,
) -> Result<(
    Box<dyn SttProvider>,
    &'static str,
    subtitler_pipeline::RunConfig,
)> {
    let key = api_key(shared.provider.env_key())
        .with_context(|| format!("missing {}", shared.provider.env_key()))?;
    let stt: Box<dyn SttProvider> = match shared.provider {
        ProviderArg::Groq => Box::new(GroqProvider::new(key)),
        ProviderArg::Openrouter => Box::new(OpenRouterProvider::new(key)),
    };
    let stt_settings = SttSettings {
        model: shared.stt_model.clone().unwrap_or_else(|| {
            SttSettings::default_model_for(shared.provider.kind()).to_string()
        }),
        language: shared.language.clone(),
        temperature: 0.0,
        defenses: SttSettings::default().defenses,
        vad: subtitler_core::config::VadSettings {
            enabled: !shared.no_vad,
            ..Default::default()
        },
        ..SttSettings::default_for(shared.provider.kind())
    };
    let profile = match &shared.profile {
        Some(path) => toml::from_str::<Profile>(&std::fs::read_to_string(path)?)
            .with_context(|| format!("parsing profile {}", path.display()))?,
        None => Profile::default(),
    };
    let cleanup = if shared.cleanup {
        Some(subtitler_pipeline::CleanupSetup {
            prompt_text: match &shared.cleanup_prompt {
                Some(path) => Some(
                    std::fs::read_to_string(path)
                        .with_context(|| format!("reading cleanup prompt {}", path.display()))?,
                ),
                None => None,
            },
            max_deletion_ratio: shared.cleanup_ratio,
        })
    } else {
        None
    };
    let cfg = subtitler_pipeline::RunConfig {
        stt: stt_settings,
        llm: LlmSettings::default(),
        langs,
        user_translation_prompt: prompt,
        translation_mode: match shared.mode {
            ModeArg::Fast => subtitler_core::config::TranslationMode::Fast,
            ModeArg::Pro => subtitler_core::config::TranslationMode::Pro,
        },
        profile,
        max_flac_mb: shared.max_flac_mb,
        target_chunk_secs: shared.target_chunk_secs,
        max_chunk_secs: shared.max_chunk_secs,
        stt_max_request_secs: shared.provider.max_request_secs(),
        noise_db: shared.noise_db.clone(),
        min_silence_secs: shared.min_silence_secs,
        rule_cleanup: !shared.no_rules,
        cleanup,
        prompts_dir: PathBuf::from("prompts"),
        cache_path: (!shared.no_cache).then(|| PathBuf::from("data/store.db")),
        trim_start_ms: shared.trim_start_secs * 1000,
        trim_end_ms: shared.trim_end_secs * 1000,
    };
    Ok((stt, shared.provider.name(), cfg))
}

// Bundled to keep `flow` under clippy's argument limit.
struct FlowRun<'a> {
    input: &'a Path,
    shared: &'a SharedArgs,
    stt: Box<dyn SttProvider>,
    stt_name: &'static str,
    llm: Option<Box<dyn subtitler_translate::LlmProvider>>,
    llm_name: Option<String>,
    cfg: subtitler_pipeline::RunConfig,
    label: &'static str,
}

async fn flow(run: FlowRun<'_>) -> Result<()> {
    let FlowRun {
        input,
        shared,
        stt,
        stt_name,
        llm,
        llm_name,
        cfg,
        label,
    } = run;
    println!(
        "[{label}] {} → {} ({}, {})",
        input.display(),
        stt_name,
        cfg.stt.model,
        if cfg.langs.is_empty() {
            "source only".to_string()
        } else {
            format!("langs: {}", cfg.langs.join(","))
        }
    );
    let artifacts = subtitler_pipeline::run_file(
        input,
        &shared.out,
        stt.as_ref(),
        llm.as_deref(),
        &cfg,
        Some(&subtitler_pipeline::StageReporter::console()),
    )
    .await?;
    println!(
        "cues: {} ({} flagged), words: {}",
        artifacts.source.cues.len(),
        artifacts.source.flags.len(),
        artifacts.words.len()
    );
    for t in &artifacts.translations {
        let failed = t
            .flags
            .iter()
            .filter(|f| matches!(f.flag, CueFlag::TranslationFailed))
            .count();
        println!(
            "translation [{}]: {} cues{}",
            t.lang,
            t.cues.len(),
            if failed > 0 {
                format!(", {failed} FAILED (source text kept)")
            } else {
                String::new()
            }
        );
    }
    for p in &artifacts.srt_paths {
        println!("wrote {}", p.1.display());
    }

    if shared.preview {
        let layout = Layout {
            max_chars_per_line: cfg.profile.max_chars_per_line as usize,
            max_lines: cfg.profile.max_lines as usize,
        };
        let mut tracks = vec![preview::track_from(
            0,
            &artifacts.source.lang,
            &cfg.stt.model,
            &format!("kaynak parça — {stt_name}"),
            &artifacts.source,
            artifacts.words.clone(),
            layout,
        )];
        for (i, t) in artifacts.translations.iter().enumerate() {
            tracks.push(preview::track_from(
                i + 1,
                &t.lang,
                &cfg.llm.model,
                &format!(
                    "çeviri — {}{}",
                    llm_name.as_deref().unwrap_or("llm"),
                    if cfg.cleanup.is_some() {
                        " (temizlik sonrası)"
                    } else {
                        ""
                    }
                ),
                t,
                Vec::new(),
                layout,
            ));
        }
        let path = preview::write_preview(&shared.out, input, &tracks)?;
        println!("preview written to {} (open in a browser)", path.display());
    }
    Ok(())
}
