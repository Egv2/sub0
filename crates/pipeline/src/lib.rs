//! Stage orchestration shared by the CLI and the desktop app.
//!
//! `run_file` is re-entrant: finished artifacts (`words.json`, `*.track.json`)
//! are reused, so a crashed job resumes without paying for STT/LLM calls again.

pub mod naming;
pub mod paths;
pub mod queue;

use std::path::{Path, PathBuf};

use subtitler_core::config::{LlmSettings, SttSettings, TranslationMode};
use subtitler_core::job::{JobEvent, JobId, JobStage};
use subtitler_core::model::{Track, Word};
use subtitler_export::Layout;
use subtitler_media as media;
use subtitler_segment::{segment, Profile};
use subtitler_stt::{
    shift_transcription_times, AudioChunk, AudioFormat, SttProvider, TranscribeOptions,
};
use subtitler_translate::{translate_track, LlmProvider};

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunConfig {
    pub stt: SttSettings,
    pub llm: LlmSettings,
    /// Empty = source-language SRT only.
    pub langs: Vec<String>,
    pub user_translation_prompt: String,
    pub translation_mode: TranslationMode,
    pub profile: Profile,
    pub max_flac_mb: u64,
    pub target_chunk_secs: u32,
    pub max_chunk_secs: u32,
    /// OpenRouter silently drops word timestamps on long requests; Groq copes
    /// with 6+ minutes.
    pub stt_max_request_secs: u32,
    pub noise_db: String,
    pub min_silence_secs: f64,
    pub rule_cleanup: bool,
    pub cleanup: Option<CleanupSetup>,
    pub prompts_dir: PathBuf,
    /// `None` disables caching.
    pub cache_path: Option<PathBuf>,
    /// Drop words near the edges, e.g. a music intro Whisper hallucinates on.
    pub trim_start_ms: u32,
    pub trim_end_ms: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CleanupSetup {
    /// `None` loads `cleanup.{lang}.md`, falling back to `cleanup.en.md`.
    pub prompt_text: Option<String>,
    pub max_deletion_ratio: f32,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            stt: SttSettings::default(),
            llm: LlmSettings::default(),
            langs: Vec::new(),
            user_translation_prompt: String::new(),
            translation_mode: TranslationMode::default(),
            profile: Profile::default(),
            max_flac_mb: 25,
            target_chunk_secs: 600,
            max_chunk_secs: 900,
            stt_max_request_secs: 600,
            noise_db: "-35dB".into(),
            min_silence_secs: 0.4,
            rule_cleanup: true,
            cleanup: None,
            prompts_dir: PathBuf::from("prompts"),
            cache_path: Some(PathBuf::from("data/store.db")),
            trim_start_ms: 0,
            trim_end_ms: 0,
        }
    }
}

#[derive(Debug)]
pub struct RunArtifacts {
    pub dir: PathBuf,
    pub words: Vec<Word>,
    pub source: Track,
    pub translations: Vec<Track>,
    /// The source transcript comes with an empty language code.
    pub srt_paths: Vec<(String, PathBuf)>,
    /// Translations only; the source track is in `source`.
    pub translation_paths: Vec<(String, PathBuf)>,
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    Media(#[from] media::MediaError),
    #[error(transparent)]
    Stt(#[from] subtitler_stt::SttError),
    #[error(transparent)]
    Export(#[from] subtitler_export::ExportError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("translation requested but no LLM provider was configured")]
    LlmRequired,
    #[error("translation for `{0}` produced a track violating the timecode invariant")]
    TimecodesChanged(String),
    #[error("input has no audio stream")]
    NoAudio,
    #[error(transparent)]
    Cleanup(#[from] subtitler_cleanup::CleanupError),
    #[error(transparent)]
    Store(#[from] subtitler_store::StoreError),
    #[error(
        "yt-dlp not found in PATH — the queue cannot download URLs \
         (install it first, e.g. `brew install yt-dlp` or `pipx install yt-dlp`)"
    )]
    YtdlpMissing,
}

impl PipelineError {
    /// Transient media/STT/LLM failures are requeued, everything else is final.
    pub fn failure_class(&self) -> media::FailureClass {
        match self {
            PipelineError::Media(e) => media::failure_class(e),
            PipelineError::Stt(e) if e.is_transient() => media::FailureClass::Transient,
            PipelineError::Cleanup(subtitler_cleanup::CleanupError::Llm(e)) if e.is_transient() => {
                media::FailureClass::Transient
            }
            _ => media::FailureClass::Permanent,
        }
    }
}

/// Writes the stage checkpoint and broadcasts the job event, so resume and
/// the UI read the same thing.
#[derive(Clone, Default)]
pub struct StageReporter {
    store: Option<std::sync::Arc<subtitler_store::Store>>,
    job: Option<JobId>,
    events: Option<tokio::sync::broadcast::Sender<JobEvent>>,
}

impl StageReporter {
    pub fn for_job(
        store: std::sync::Arc<subtitler_store::Store>,
        job: JobId,
        events: tokio::sync::broadcast::Sender<JobEvent>,
    ) -> Self {
        Self {
            store: Some(store),
            job: Some(job),
            events: Some(events),
        }
    }

    pub fn console() -> Self {
        Self::default()
    }

    pub async fn report(&self, stage: JobStage) {
        if let Some(job) = self.job {
            if let Some(store) = &self.store {
                let name = serde_json::to_string(&stage)
                    .map(|s| s.trim_matches('"').to_string())
                    .unwrap_or_else(|_| format!("{stage:?}").to_lowercase());
                let _ = store.set_job_stage(job.0 as i64, &name).await;
            }
            if let Some(tx) = &self.events {
                let _ = tx.send(JobEvent::StageStarted { job, stage });
            }
        } else {
            println!("▶ {stage:?}");
        }
    }
}

pub async fn run_file(
    input: &Path,
    out_dir: &Path,
    stt: &dyn SttProvider,
    llm: Option<&dyn LlmProvider>,
    cfg: &RunConfig,
    reporter: Option<&StageReporter>,
) -> Result<RunArtifacts, PipelineError> {
    let reporter = reporter.cloned().unwrap_or_default();
    let stage = |s: JobStage| reporter.report(s);
    let paths = paths::JobPaths::new(out_dir);
    std::fs::create_dir_all(paths.out())?;
    paths.migrate_legacy()?;
    let cache = match &cfg.cache_path {
        Some(path) => Some(subtitler_store::Store::open(path).await?),
        None => None,
    };

    let words: Vec<Word> = match read_json(&paths.words())? {
        Some(words) => {
            stage(JobStage::Transcribing).await; // already done; report for flow
            words
        }
        None => {
            stage(JobStage::Extracting).await;
            let info = media::probe(input).await?;
            if !info.has_audio {
                return Err(PipelineError::NoAudio);
            }

            // Chunk only when a provider limit requires it.
            let full_flac = paths.temp_audio();
            if !full_flac.exists() {
                paths.ensure_temp()?;
                media::extract_audio(input, &full_flac, media::AudioExtractFormat::Flac).await?;
            }
            let flac_mb = std::fs::metadata(&full_flac)?.len() as f64 / (1024.0 * 1024.0);

            let speech = if cfg.stt.vad.enabled {
                detect_speech_ranges(&full_flac, &cfg.stt.vad).await
            } else {
                None
            };
            if let Some(ranges) = &speech {
                write_json(&paths.speech_ranges(), ranges)?;
            }

            stage(JobStage::Chunking).await;
            let chunks: Vec<(u32, u32, PathBuf)> = if flac_mb <= cfg.max_flac_mb as f64
                && info.duration_ms <= cfg.stt_max_request_secs * 1000
            {
                vec![(0, info.duration_ms, full_flac.clone())]
            } else {
                let silences = match &speech {
                    Some(ranges) => media::vad::gaps(ranges, info.duration_ms),
                    None => {
                        media::detect_silences(
                            &full_flac,
                            &cfg.noise_db,
                            cfg.min_silence_secs,
                            Some(info.duration_ms),
                        )
                        .await?
                    }
                };
                let target_ms = cfg.target_chunk_secs.min(cfg.stt_max_request_secs) * 1000;
                let cap_ms = cfg.max_chunk_secs.min(cfg.stt_max_request_secs) * 1000;
                media::plan_chunks(info.duration_ms, &silences, target_ms, cap_ms)
                    .into_iter()
                    .map(|plan| {
                        let path = paths.temp_chunk(plan.start_ms);
                        (plan.start_ms, plan.end_ms, path)
                    })
                    .collect()
            };
            for (start, end, path) in &chunks {
                if chunks.len() > 1 && !path.exists() {
                    media::extract_audio_slice(
                        input,
                        path,
                        *start,
                        *end,
                        media::AudioExtractFormat::Flac,
                    )
                    .await?;
                }
            }

            stage(JobStage::Transcribing).await;
            let mut opts = TranscribeOptions {
                model: cfg.stt.model.clone(),
                language: cfg.stt.language.clone(),
                temperature: cfg.stt.temperature,
                prompt: cfg
                    .stt
                    .language
                    .as_deref()
                    .and_then(|l| load_stt_prompt(&cfg.prompts_dir, l)),
            };
            if opts.language.is_none() && has_stt_prompts(&cfg.prompts_dir) {
                let probe_start = speech.as_ref().and_then(|r| r.first()).map_or(0, |r| r.start_ms);
                let probe = probe_language(
                    stt,
                    &opts,
                    &full_flac,
                    &paths,
                    cache.as_ref(),
                    (probe_start, info.duration_ms),
                )
                .await;
                match probe {
                    Ok(Some(lang)) => opts.prompt = load_stt_prompt(&cfg.prompts_dir, &lang),
                    Ok(None) => {}
                    Err(e) => eprintln!("[stt] language probe failed ({e}); transcribing without a prompt"),
                }
            }
            let mut words: Vec<Word> = Vec::new();
            let mut segments: Vec<subtitler_stt::Segment> = Vec::new();
            let mut detected_lang: Option<String> = None;
            let mut stt_cache_hits = 0u32;
            for (start, end, path) in &chunks {
                let bytes = tokio::fs::read(path).await?;
                let audio_hash = subtitler_store::Store::hash_hex(&bytes);
                let chunk = AudioChunk {
                    bytes,
                    format: AudioFormat::Flac,
                    offset_ms: *start,
                    duration_ms: end - start,
                };
                let stt_key = cache.as_ref().map(|_store| {
                    subtitler_store::Store::cache_key(&[
                        "stt",
                        stt.name(),
                        &opts.model,
                        opts.language.as_deref().unwrap_or("auto"),
                        &opts.temperature.to_string(),
                        opts.prompt.as_deref().unwrap_or(""),
                        &audio_hash,
                    ])
                });
                let mut t = None;
                if let (Some(store), Some(key)) = (cache.as_ref(), stt_key.as_deref()) {
                    if let Ok(Some(json)) = store.stt_get(key).await {
                        if let Ok(cached) =
                            serde_json::from_str::<subtitler_stt::Transcription>(&json)
                        {
                            stt_cache_hits += 1;
                            t = Some(cached);
                        }
                    }
                }
                let mut t = match t {
                    Some(t) => t,
                    None => {
                        let t = retry_transcribe(stt, &chunk, &opts).await?;
                        if let (Some(store), Some(key)) = (cache.as_ref(), stt_key.as_deref()) {
                            if let Ok(json) = serde_json::to_string(&t) {
                                let _ = store.stt_put(key, &json).await;
                            }
                        }
                        t
                    }
                };
                if detected_lang.is_none() {
                    detected_lang = t.language.clone();
                }
                shift_transcription_times(&mut t, *start);
                words.extend(t.words.iter().cloned());
                segments.extend(t.segments.iter().cloned());
            }
            if stt_cache_hits > 0 {
                println!(
                    "stt cache: {stt_cache_hits}/{} chunk(s) served from cache",
                    chunks.len()
                );
            }
            words.sort_by_key(|w| w.start_ms);

            stage(JobStage::Validating).await;
            let trim = subtitler_stt::TrimRange::new(
                cfg.trim_start_ms,
                Some(info.duration_ms.saturating_sub(cfg.trim_end_ms)),
            );
            let (kept_words, _kept_segments, report) =
                subtitler_stt::apply_defenses(&words, &segments, &cfg.stt.defenses, &trim);
            if !report.dropped_segments.is_empty()
                || !report.dropped_words.is_empty()
                || report.collapsed_segments > 0
            {
                println!(
                    "defenses: {} segment(s) dropped, {} collapsed, {} word(s) removed",
                    report.dropped_segments.len(),
                    report.collapsed_segments,
                    report.dropped_words.len()
                );
                write_json(&paths.dropped_segments(), &report)?;
            }
            words = kept_words;

            if let Some(ranges) = &speech {
                words = filter_by_speech(words, ranges, &cfg.stt.vad, &paths)?;
            }

            let lang = normalize_lang_code(
                cfg.stt
                    .language
                    .clone()
                    .or(detected_lang)
                    .unwrap_or_else(|| "und".into())
                    .as_str(),
            );
            // Keeps the detected language around if a later stage crashes.
            write_json(
                &paths.meta(),
                &serde_json::json!({ "lang": lang }),
            )?;

            // Audio is the biggest thing a job produces and is useless past this
            // point, so drop it now, even if a later stage fails.
            let _ = paths.remove_temp();

            if cfg.rule_cleanup {
                let deleted = subtitler_cleanup::rules::rule_deletions(&words, &lang);
                if !deleted.is_empty() {
                    println!("rules: {} of {} words deleted", deleted.len(), words.len());
                    let removed: Vec<&Word> = deleted.iter().map(|&i| &words[i]).collect();
                    write_json(&paths.rules_deleted(), &removed)?;
                    words = remove_indices(words, &deleted);
                }
            }

            // Before segmentation, so cue boundaries come from the cleaned words.
            if let Some(setup) = &cfg.cleanup {
                stage(JobStage::Cleaning).await;
                let llm = llm.ok_or(PipelineError::LlmRequired)?;
                let prompt = match &setup.prompt_text {
                    Some(text) => text.clone(),
                    None => load_cleanup_prompt(&cfg.prompts_dir, &lang)?,
                };
                let result = subtitler_cleanup::cleanup_words(
                    &words,
                    &subtitler_cleanup::CleanupOptions {
                        provider: llm,
                        model: &cfg.llm.model,
                        prompt: &prompt,
                        lang: &lang,
                        max_deletion_ratio: setup.max_deletion_ratio,
                        cache: cache.as_ref(),
                    },
                )
                .await?;
                println!(
                    "cleanup: {} of {} words deleted, {} window(s) flagged; cache {} hit(s)/{} call(s)",
                    result.deleted_indices.len(),
                    words.len(),
                    result.flagged_windows.len(),
                    result.stats.cache_hits,
                    result.stats.llm_calls
                );
                write_json(
                    &paths.cleanup_deleted(),
                    &result.deleted_indices,
                )?;
                words = result.kept;
            }
            write_json(&paths.words(), &words)?;
            words
        }
    };

    stage(JobStage::Segmenting).await;
    let source: Track = match read_json(&paths.source_track())? {
        Some(track) => track,
        None => {
            let meta: serde_json::Value =
                read_json(&paths.meta())?.unwrap_or(serde_json::json!({}));
            let lang = meta["lang"].as_str().unwrap_or("und").to_string();
            let fps = media::probe(input).await.ok().and_then(|i| i.fps);
            let track = segment(&words, &cfg.profile, &lang, fps);
            write_json(&paths.source_track(), &track)?;
            track
        }
    };

    let layout = Layout {
        max_chars_per_line: cfg.profile.max_chars_per_line as usize,
        max_lines: cfg.profile.max_lines as usize,
    };
    let mut srt_paths = Vec::new();
    let mut translation_paths = Vec::new();
    let source_srt = paths.srt(&source.lang);
    subtitler_export::write_srt(&source, layout, &source_srt)?;
    srt_paths.push((String::new(), source_srt));

    let mut translations = Vec::new();
    if !cfg.langs.is_empty() {
        let llm = llm.ok_or(PipelineError::LlmRequired)?;
        let mut brief: Option<subtitler_translate::Brief> = None;
        let mut brief_tried = false;
        for target in &cfg.langs {
            // Detected language can match a target picked before the video existed.
            // Translating into itself only trips the echo check and burns retries.
            if target == &source.lang {
                println!(
                    "translation atlandı: hedef dil kaynak dil ile aynı ({target}) — \
                     bu dil çeviri değil, kaynak transcript'inin kopyası olur"
                );
                continue;
            }
            stage(JobStage::Translating).await;
            let track_path = paths.translation_track(target);
            let translated = match read_json::<Track>(&track_path)? {
                Some(track) => track,
                None => {
                    if cfg.translation_mode == TranslationMode::Pro && !brief_tried {
                        brief_tried = true;
                        brief = load_or_create_brief(&paths, &source, llm, cfg, cache.as_ref()).await?;
                    }
                    let (translated, stats) = translate_track(
                        &source,
                        subtitler_translate::TranslateOptions {
                            provider: llm,
                            model: &cfg.llm.model,
                            target_lang: target,
                            user_prompt: &cfg.user_translation_prompt,
                            cache: cache.as_ref(),
                            brief: brief.as_ref(),
                            prior_context: cfg.translation_mode == TranslationMode::Pro,
                        },
                    )
                    .await;
                    println!(
                        "translation [{target}] cache: {} hit(s), {} LLM call(s)",
                        stats.cache_hits, stats.llm_calls
                    );
                    write_json(&track_path, &translated)?;
                    translated
                }
            };
            stage(JobStage::Validating).await;
            subtitler_core::assert_timecodes_preserved(&source, &translated)
                .map_err(|_| PipelineError::TimecodesChanged(target.clone()))?;
            let path = paths.srt(target);
            subtitler_export::write_srt_checked(&source, &translated, layout, &path)?;
            srt_paths.push((target.clone(), path));
            translation_paths.push((target.clone(), track_path));
            translations.push(translated);
        }
    }
    stage(JobStage::Exporting).await;

    Ok(RunArtifacts {
        dir: out_dir.to_path_buf(),
        words,
        source,
        translations,
        srt_paths,
        translation_paths,
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, PipelineError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), PipelineError> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

async fn retry_transcribe(
    stt: &dyn SttProvider,
    chunk: &AudioChunk,
    opts: &TranscribeOptions,
) -> Result<subtitler_stt::Transcription, subtitler_stt::SttError> {
    let mut attempt = 0;
    loop {
        match stt.transcribe_chunk(chunk, opts).await {
            Ok(t) => return Ok(t),
            Err(e) if e.is_transient() && attempt < 2 => {
                tokio::time::sleep(std::time::Duration::from_secs(1 + attempt)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Groq reports display names ("Turkish"), OpenRouter ISO codes ("tr").
fn normalize_lang_code(lang: &str) -> String {
    let code = subtitler_core::lang::from_display_name(lang);
    if subtitler_core::lang::is_supported(code) {
        code.to_string()
    } else {
        "und".to_string()
    }
}

async fn load_or_create_brief(
    paths: &paths::JobPaths,
    source: &Track,
    llm: &dyn LlmProvider,
    cfg: &RunConfig,
    cache: Option<&subtitler_store::Store>,
) -> Result<Option<subtitler_translate::Brief>, PipelineError> {
    if let Some(brief) = read_json(&paths.brief())? {
        return Ok(Some(brief));
    }
    let catalog = subtitler_translate::StyleCatalog::load(&cfg.prompts_dir.join("styles"));
    let brief =
        subtitler_translate::generate_brief(llm, &cfg.llm.model, source, &catalog, cache).await;
    match &brief {
        Some(b) => {
            println!("brief: {} [{}]", b.media_type, b.styles.join(", "));
            write_json(&paths.brief(), b)?;
        }
        None => eprintln!("[brief] unavailable; translating without localization brief"),
    }
    Ok(brief)
}

const PROBE_MS: u32 = 30_000;

fn has_stt_prompts(prompts_dir: &Path) -> bool {
    std::fs::read_dir(prompts_dir).into_iter().flatten().flatten().any(|e| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        name.starts_with("stt.") && name.ends_with(".txt")
    })
}

/// Detects the language from a short clip so the very first chunk can get the
/// matching Whisper prompt. Costs ~30 s of extra audio per file.
async fn probe_language(
    stt: &dyn SttProvider,
    opts: &TranscribeOptions,
    full_flac: &Path,
    paths: &paths::JobPaths,
    cache: Option<&subtitler_store::Store>,
    (start_ms, duration_ms): (u32, u32),
) -> Result<Option<String>, PipelineError> {
    let start_ms = start_ms.min(duration_ms.saturating_sub(1));
    let end_ms = (start_ms + PROBE_MS).min(duration_ms);
    if end_ms <= start_ms {
        return Ok(None);
    }
    let probe = paths.temp_dir().join("probe.flac");
    media::extract_audio_slice(full_flac, &probe, start_ms, end_ms, media::AudioExtractFormat::Flac)
        .await?;
    let chunk = AudioChunk {
        bytes: tokio::fs::read(&probe).await?,
        format: AudioFormat::Flac,
        offset_ms: 0,
        duration_ms: end_ms - start_ms,
    };
    let _ = std::fs::remove_file(&probe);
    let key = subtitler_store::Store::cache_key(&[
        "stt-probe",
        stt.name(),
        &opts.model,
        &subtitler_store::Store::hash_hex(&chunk.bytes),
    ]);
    if let Some(store) = cache {
        if let Ok(Some(lang)) = store.stt_get(&key).await {
            return Ok(Some(lang).filter(|l| !l.is_empty()));
        }
    }
    let t = retry_transcribe(stt, &chunk, &TranscribeOptions { prompt: None, ..opts.clone() }).await?;
    let lang = t.language.as_deref().map(normalize_lang_code).filter(|l| l != "und");
    if let Some(store) = cache {
        let _ = store.stt_put(&key, lang.as_deref().unwrap_or("")).await;
    }
    Ok(lang)
}

/// A VAD failure falls back to ffmpeg silence detection rather than failing
/// the job.
async fn detect_speech_ranges(
    audio: &Path,
    vad: &subtitler_core::config::VadSettings,
) -> Option<Vec<media::SpeechRange>> {
    let params = media::VadParams {
        threshold: vad.threshold,
        min_speech_ms: vad.min_speech_ms,
        min_silence_ms: vad.min_silence_ms,
        speech_pad_ms: vad.speech_pad_ms,
    };
    match media::detect_speech(audio, params).await {
        Ok(ranges) if !ranges.is_empty() => Some(ranges),
        Ok(_) => {
            eprintln!("[vad] no speech detected; VAD filtering skipped");
            None
        }
        Err(e) => {
            eprintln!("[vad] failed ({e}); falling back to ffmpeg silence detection");
            None
        }
    }
}

/// Dropping more than this is more likely a VAD miss (singing, music bed)
/// than a hallucinated transcript.
const VAD_MAX_DROP_RATIO: f32 = 0.6;

fn filter_by_speech(
    words: Vec<Word>,
    ranges: &[media::SpeechRange],
    vad: &subtitler_core::config::VadSettings,
    paths: &paths::JobPaths,
) -> Result<Vec<Word>, PipelineError> {
    let in_speech: Vec<bool> = words
        .iter()
        .map(|w| media::vad::overlaps_speech(ranges, w.start_ms, w.end_ms, vad.word_tolerance_ms))
        .collect();
    let total = words.len();
    let dropped_n = in_speech.iter().filter(|k| !**k).count();
    if dropped_n == 0 {
        return Ok(words);
    }
    if dropped_n as f32 / total as f32 > VAD_MAX_DROP_RATIO {
        eprintln!("[vad] would drop {dropped_n} of {total} words; skipping the VAD filter");
        return Ok(words);
    }
    let mut kept = Vec::with_capacity(total - dropped_n);
    let mut dropped = Vec::with_capacity(dropped_n);
    for (w, keep) in words.into_iter().zip(in_speech) {
        if keep { kept.push(w) } else { dropped.push(w) }
    }
    println!("vad: {} of {total} words outside speech removed", dropped.len());
    write_json(&paths.vad_dropped(), &dropped)?;
    Ok(kept)
}

fn remove_indices(words: Vec<Word>, sorted: &[usize]) -> Vec<Word> {
    words
        .into_iter()
        .enumerate()
        .filter(|(i, _)| sorted.binary_search(i).is_err())
        .map(|(_, w)| w)
        .collect()
}

/// Whisper imitates the prompt's style, so a clean sample suppresses fillers
/// and sound captions.
fn load_stt_prompt(prompts_dir: &Path, lang: &str) -> Option<String> {
    std::fs::read_to_string(prompts_dir.join(format!("stt.{lang}.txt")))
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

fn load_cleanup_prompt(prompts_dir: &Path, lang: &str) -> Result<String, PipelineError> {
    for candidate in [format!("cleanup.{lang}.md"), "cleanup.en.md".into()] {
        let path = prompts_dir.join(candidate);
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Ok(text);
        }
    }
    Err(PipelineError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "cleanup prompt not found: {}/cleanup.{lang}.md (or cleanup.en.md)",
            prompts_dir.display()
        ),
    )))
}
