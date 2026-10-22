//! Tauri commands. Thin wrappers over the library crates; no pipeline logic here.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use subtitler_core::config::{LlmSettings, ProviderKind, SttSettings};
use subtitler_core::job::{JobEvent, JobId};
use subtitler_core::lang::{self, Language};
use subtitler_pipeline::{queue, CleanupSetup, RunConfig, StageReporter};
use subtitler_store::Store;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

mod catalog;
mod gc;


/// Nearest ancestor containing `crates/` (dev CWD is `app/src-tauri`).
/// `SUBTITLER_ROOT` overrides.
fn repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("SUBTITLER_ROOT") {
        return PathBuf::from(root);
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .ancestors()
        .find(|p| p.join("crates").is_dir())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn data_dir() -> PathBuf {
    repo_root().join("data")
}

fn absolute(path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        repo_root().join(p)
    }
}


/// Persisted as `data/settings.toml`. API keys live in `data/keys.json`
/// instead, so this file is safe to share.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub stt_provider: String,
    pub stt_model: String,
    pub llm_provider: String,
    pub llm_model: String,
    pub langs: Vec<String>,
    pub translate: bool,
    pub translation_prompt: String,
    /// "fast" | "pro"
    pub translation_mode: String,
    pub cleanup: bool,
    pub cleanup_ratio: f32,
    pub trim_start_secs: u32,
    pub trim_end_secs: u32,
    pub target_chunk_secs: u32,
    pub max_chunk_secs: u32,
    pub max_flac_mb: u64,
    pub noise_db: String,
    pub min_silence_secs: f64,
    pub out_dir: String,
    pub profile: String,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            stt_provider: "groq".into(),
            stt_model: SttSettings::default_model_for(ProviderKind::Groq).into(),
            llm_provider: "openrouter".into(),
            llm_model: "deepseek/deepseek-v4.1-flash".into(),
            langs: vec!["en".into()],
            translate: true,
            translation_prompt: String::new(),
            translation_mode: "pro".into(),
            cleanup: true,
            cleanup_ratio: 0.2,
            trim_start_secs: 0,
            trim_end_secs: 0,
            target_chunk_secs: 600,
            max_chunk_secs: 900,
            max_flac_mb: 25,
            noise_db: "-35dB".into(),
            min_silence_secs: 0.4,
            out_dir: "data/batch".into(),
            profile: "netflix".into(),
        }
    }
}

impl UiSettings {
    fn settings_path() -> PathBuf {
        data_dir().join("settings.toml")
    }

    fn load() -> Self {
        let mut settings: Self = std::fs::read_to_string(Self::settings_path())
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        settings.repair_stt_model();
        settings.repair_langs();
        settings
    }

    fn parse_provider(name: &str) -> ProviderKind {
        if name == "openrouter" {
            ProviderKind::OpenRouter
        } else {
            ProviderKind::Groq
        }
    }

    /// Repairs a slug left over from a provider switch. OpenRouter answers 400
    /// to Groq's bare `whisper-large-v3` instead of "unknown model".
    fn repair_stt_model(&mut self) {
        let kind = Self::parse_provider(&self.stt_provider);
        if !SttSettings::is_model_valid_for(kind, &self.stt_model) {
            self.stt_model = SttSettings::default_model_for(kind).into();
        }
    }

    fn repair_langs(&mut self) {
        let kept: Vec<String> = self
            .langs
            .iter()
            .filter_map(|code| lang::by_code(code).map(|l| l.code.to_string()))
            .collect();
        let mut seen: Vec<String> = Vec::new();
        self.langs = kept
            .into_iter()
            .filter(|code| {
                if seen.contains(code) {
                    return false;
                }
                seen.push(code.clone());
                true
            })
            .collect();
        // No language selected means no translation.
        if self.langs.is_empty() {
            self.translate = false;
        }
    }

    fn provider_env(kind: ProviderKind) -> &'static str {
        match kind {
            ProviderKind::Groq => "GROQ_API_KEY",
            ProviderKind::OpenRouter => "OPENROUTER_API_KEY",
        }
    }

    fn key_error(&self, kind: ProviderKind) -> String {
        format!(
            "missing {} — paste it in Settings ▸ Sağlayıcılar or set the environment variable",
            Self::provider_env(kind)
        )
    }

    /// Mirrors the CLI's config build.
    fn to_run_config(&self) -> RunConfig {
        RunConfig {
            stt: SttSettings {
                provider: Self::parse_provider(&self.stt_provider),
                model: self.stt_model.clone(),
                language: None,
                temperature: 0.0,
                defenses: Default::default(),
                vad: Default::default(),
            },
            llm: LlmSettings {
                provider: Self::parse_provider(&self.llm_provider),
                model: self.llm_model.clone(),
                temperature: 0.0,
            },
            langs: if self.translate { self.langs.clone() } else { Vec::new() },
            user_translation_prompt: self.translation_prompt.clone(),
            translation_mode: if self.translation_mode == "fast" {
                subtitler_core::config::TranslationMode::Fast
            } else {
                subtitler_core::config::TranslationMode::Pro
            },
            profile: subtitler_segment::Profile::default(),
            max_flac_mb: self.max_flac_mb,
            target_chunk_secs: self.target_chunk_secs,
            max_chunk_secs: self.max_chunk_secs,
            stt_max_request_secs: if self.stt_provider == "openrouter" { 300 } else { 600 },
            noise_db: self.noise_db.clone(),
            min_silence_secs: self.min_silence_secs,
            rule_cleanup: true,
            cleanup: self.cleanup.then(|| CleanupSetup {
                prompt_text: None,
                max_deletion_ratio: self.cleanup_ratio,
            }),
            prompts_dir: repo_root().join("prompts"),
            cache_path: Some(data_dir().join("store.db")),
            trim_start_ms: self.trim_start_secs * 1000,
            trim_end_ms: self.trim_end_secs * 1000,
        }
    }

    fn build_stt(&self) -> Result<Box<dyn subtitler_stt::SttProvider>, String> {
        let kind = Self::parse_provider(&self.stt_provider);
        let key = resolve_key(kind).ok_or_else(|| self.key_error(kind))?;
        Ok(match kind {
            ProviderKind::Groq => Box::new(subtitler_stt::GroqProvider::new(key)),
            ProviderKind::OpenRouter => Box::new(subtitler_stt::OpenRouterProvider::new(key)),
        })
    }

    fn build_llm(&self) -> Result<Option<Box<dyn subtitler_translate::LlmProvider>>, String> {
        if self.langs.is_empty() && !self.cleanup {
            return Ok(None);
        }
        let kind = Self::parse_provider(&self.llm_provider);
        let key = resolve_key(kind).ok_or_else(|| self.key_error(kind))?;
        Ok(Some(match kind {
            ProviderKind::Groq => Box::new(subtitler_translate::OpenAiCompatLlm::groq(key)),
            ProviderKind::OpenRouter => {
                Box::new(subtitler_translate::OpenAiCompatLlm::openrouter(key))
            }
        }))
    }
}


/// API keys in `data/keys.json`, owner-only on unix.
#[derive(Debug, Default, Serialize, Deserialize)]
struct KeyStore {
    #[serde(default)]
    groq: Option<String>,
    #[serde(default)]
    openrouter: Option<String>,
}

impl KeyStore {
    fn path() -> PathBuf {
        data_dir().join("keys.json")
    }

    fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn get(&self, kind: ProviderKind) -> Option<&str> {
        let raw = match kind {
            ProviderKind::Groq => self.groq.as_deref(),
            ProviderKind::OpenRouter => self.openrouter.as_deref(),
        };
        raw.map(str::trim).filter(|v| !v.is_empty())
    }

    /// Blank input clears the entry instead of storing an empty secret.
    fn set(&mut self, kind: ProviderKind, value: Option<&str>) {
        let slot = match kind {
            ProviderKind::Groq => &mut self.groq,
            ProviderKind::OpenRouter => &mut self.openrouter,
        };
        *slot = value
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
    }
}

/// Stored key first, then the environment. A blank env value never shadows
/// a stored key.
fn resolve_key(kind: ProviderKind) -> Option<String> {
    if let Some(stored) = KeyStore::load().get(kind) {
        return Some(stored.to_string());
    }
    std::env::var(UiSettings::provider_env(kind))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn key_source(kind: ProviderKind) -> &'static str {
    if KeyStore::load().get(kind).is_some() {
        "file"
    } else {
        std::env::var(UiSettings::provider_env(kind))
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
            .then_some("env")
            .unwrap_or("")
    }
}


/// Shared by the queue and translation runs, so the UI has one event feed.
pub struct AppState {
    pub store: Arc<Store>,
    pub queue_running: Mutex<bool>,
    pub events: broadcast::Sender<JobEvent>,
    /// Jobs with a translation in flight. `Arc` because the spawned task
    /// outlives the `State` borrow.
    pub translating: Arc<Mutex<HashSet<i64>>>,
}

#[derive(Clone)]
pub struct QueueCancel(pub CancellationToken);


#[tauri::command]
fn load_settings() -> Result<UiSettings, String> {
    match std::fs::read_to_string(UiSettings::settings_path()) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("settings.toml: {e}")),
        Err(_) => Ok(UiSettings::default()),
    }
}

#[tauri::command]
fn save_settings(mut settings: UiSettings) -> Result<(), String> {
    // Don't trust the picker: a bad code ends up in an API call and a filename.
    settings.repair_langs();
    let path = UiSettings::settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

#[tauri::command]
fn settings_path() -> String {
    UiSettings::settings_path().display().to_string()
}

#[tauri::command]
async fn create_batch(
    state: State<'_, AppState>,
    settings: UiSettings,
    inputs: Vec<String>,
) -> Result<i64, String> {
    if inputs.is_empty() {
        return Err("no inputs given".into());
    }
    let mut expanded: Vec<String> = Vec::new();
    for input in &inputs {
        if input.starts_with("http://") || input.starts_with("https://") {
            // If expansion fails, queue the raw URL and let the download stage report it.
            let urls = subtitler_media::ytdlp::expand_playlist(input)
                .await
                .unwrap_or_else(|_| vec![input.clone()]);
            expanded.extend(urls);
        } else {
            if !Path::new(input).is_file() {
                return Err(format!("input file not found: {input}"));
            }
            expanded.push(input.clone());
        }
    }

    let run_cfg = settings.to_run_config();
    let settings_json = serde_json::to_string(&run_cfg).map_err(|e| e.to_string())?;
    let batch_id = state
        .store
        .create_batch(&settings_json)
        .await
        .map_err(|e| e.to_string())?;
    for input in &expanded {
        let kind = if input.starts_with("http://") || input.starts_with("https://") {
            "url"
        } else {
            "file"
        };
        let job_dir = subtitler_pipeline::naming::job_dir(
            &absolute(&settings.out_dir),
            input,
        )
        .map_err(|e| e.to_string())?;
        state
            .store
            .enqueue_job(batch_id, kind, input, &job_dir)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(batch_id)
}

/// Events go out on `job-event`, completion on `queue-done`. One queue at a time.
#[tauri::command]
async fn start_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Option<UiSettings>,
    batch_id: Option<i64>,
    limit: Option<u64>,
) -> Result<String, String> {
    {
        let mut running =
            state.queue_running.lock().map_err(|_| "queue lock poisoned".to_string())?;
        if *running {
            return Err("queue is already running".into());
        }
        *running = true;
    }

    // Retries reuse the persisted settings rather than whatever the UI shows.
    let settings = settings.unwrap_or_else(UiSettings::load);
    let store = state.store.clone();
    let cancel = CancellationToken::new();
    app.manage(QueueCancel(cancel.clone()));

    let events = state.events.clone();
    let build = || -> Result<queue::QueueRun, String> {
        Ok(queue::QueueRun {
            store: store.clone(),
            stt: settings.build_stt()?,
            llm: settings.build_llm()?,
            cfg: settings.to_run_config(),
            events: events.clone(),
            batch: batch_id,
            limit,
            prefetch_ahead: 2,
            cancel: cancel.clone(),
        })
    };
    let run = match build() {
        Ok(run) => run,
        Err(e) => {
            if let Ok(mut running) = state.queue_running.lock() {
                *running = false;
            }
            return Err(e);
        }
    };

    let app_for_done = app.clone();
    tauri::async_runtime::spawn(async move {
        let summary = queue::run_queue(run).await;
        let payload = match summary {
            Ok(summary) => serde_json::to_value(summary).unwrap_or_default(),
            Err(e) => serde_json::json!({ "error": e.to_string() }),
        };
        let _ = app_for_done.emit("queue-done", payload);
        if let Some(state) = app_for_done.try_state::<AppState>() {
            if let Ok(mut running) = state.queue_running.lock() {
                *running = false;
            }
        }
    });
    Ok("started".into())
}

#[tauri::command]
fn cancel_queue(app: AppHandle) -> Result<(), String> {
    match app.try_state::<QueueCancel>() {
        Some(cancel) => {
            cancel.0.cancel();
            Ok(())
        }
        None => Err("no queue is running".into()),
    }
}

#[tauri::command]
async fn retry_job(app: AppHandle, state: State<'_, AppState>, job_id: i64) -> Result<(), String> {
    let job = state
        .store
        .job_by_id(job_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("job {job_id} not found"))?;
    if job.status != "failed" {
        return Err("only failed jobs can be retried".into());
    }
    state.store.requeue_job(job_id).await.map_err(|e| e.to_string())?;
    start_queue(app, state, None, Some(job.batch_id), None).await?;
    Ok(())
}

/// Running jobs belong to the queue worker; cancel the queue instead.
#[tauri::command]
async fn remove_job(state: State<'_, AppState>, job_id: i64) -> Result<(), String> {
    let removed =
        state.store.remove_job(job_id).await.map_err(|e| e.to_string())?;
    if removed {
        Ok(())
    } else {
        Err(format!("job {job_id} is running or does not exist"))
    }
}

async fn batch_profile(state: &State<'_, AppState>, job: &subtitler_store::JobRow) -> Result<subtitler_segment::Profile, String> {
    let name = state
        .store
        .batch_settings(job.batch_id)
        .await
        .map_err(|e| e.to_string())?
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|v| v.get("profile").and_then(|p| p.as_str()).map(str::to_string))
        .unwrap_or_else(|| "netflix".to_string());
    Ok(subtitler_segment::Profile::resolve(&name))
}

/// The UI reads its format list from here.
#[tauri::command]
fn export_formats() -> Vec<&'static str> {
    subtitler_export::Format::ALL.iter().map(|f| f.as_str()).collect()
}

/// Paths come from the client, so each one must be a registered artifact of
/// the job. Output filenames are built from the recorded language code, never
/// from a client string.
#[tauri::command]
async fn export_tracks(
    state: State<'_, AppState>,
    job_id: i64,
    format: String,
    track_paths: Vec<String>,
) -> Result<Vec<String>, String> {
    let fmt = subtitler_export::Format::parse(&format)
        .ok_or_else(|| format!("bilinmeyen biçim: {format}"))?;
    if track_paths.is_empty() {
        return Err("en az bir dil seç".into());
    }
    let job = state
        .store
        .job_by_id(job_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("iş {job_id} bulunamadı"))?;

    let known: Vec<String> = state
        .store
        .list_artifacts(job_id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|a| a.kind == "track")
        .map(|a| a.path)
        .collect();
    for path in &track_paths {
        if !known.iter().any(|k| k == path) {
            return Err("bu altyazı bu işe ait değil".into());
        }
    }

    // Same line layout as the pipeline used.
    let layout = batch_profile(&state, &job).await?.layout();
    let out_dir = PathBuf::from(&job.out_dir);
    let mut written = Vec::new();

    for path in track_paths {
        let track: subtitler_core::model::Track =
            serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let stem = match Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|f| f.strip_suffix(".track.json"))
        {
            Some("source") | None => "kaynak".to_string(),
            Some(lang) => lang.to_string(),
        };
        // Recorded codes are safe, but check for separators anyway.
        if stem.is_empty() || stem.contains('/') || stem.contains('\\') {
            return Err(format!("geçersiz altyazı adı: {stem}"));
        }
        let target = out_dir.join(format!("{stem}.{}", fmt.as_str()));
        subtitler_export::write_format(&track, layout, fmt, &target)
            .map_err(|e| e.to_string())?;
        written.push(target.to_string_lossy().to_string());
    }
    written.sort();
    Ok(written)
}

/// Rebuilds a finished job from scratch. The pipeline reuses whatever is on
/// disk, so the old output has to go first or nothing would change. Only
/// folders under `data/` are removed (same guard as `delete_project`).
#[tauri::command]
async fn retranscribe_job(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: i64,
) -> Result<(), String> {
    let job = state
        .store
        .job_by_id(job_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("job {job_id} not found"))?;
    if state
        .translating
        .lock()
        .map_err(|_| "lock poisoned".to_string())?
        .contains(&job_id)
    {
        return Err("this job is being translated right now".into());
    }

    let out_dir = PathBuf::from(&job.out_dir);
    let canonical = out_dir.canonicalize().unwrap_or_else(|_| out_dir.clone());
    let root = data_dir().canonicalize().unwrap_or_else(|_| data_dir());
    if !canonical.starts_with(&root) || canonical == root || !canonical.is_dir() {
        return Err("job folder not found".into());
    }
    std::fs::remove_dir_all(&canonical).map_err(|e| e.to_string())?;

    // Records point at deleted files until the rerun rewrites them.
    state
        .store
        .clear_artifacts(job_id)
        .await
        .map_err(|e| e.to_string())?;

    state
        .store
        .requeue_job(job_id)
        .await
        .map_err(|e| e.to_string())?;
    start_queue(app, state, None, Some(job.batch_id), None).await?;
    Ok(())
}

/// Deletes a job and its files. Only folders under `data/` or the configured
/// output folder are removed; anything else just loses its record. A running
/// job is accepted: dropping the record is how a stuck job gets cleared.
#[tauri::command]
async fn delete_project(
    state: State<'_, AppState>,
    app: AppHandle,
    job_id: i64,
    confirm: bool,
) -> Result<(), String> {
    if !confirm {
        return Err("onay eksik".into());
    }
    if state.translating.lock().map_err(|_| "lock poisoned".to_string())?.contains(&job_id) {
        return Err("bu iş şu anda çevriliyor".into());
    }
    let job = state
        .store
        .job_by_id(job_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("iş {job_id} bulunamadı"))?;

    let removed = if job.status == "running" {
        state.store.cancel_job(job_id).await.map_err(|e| e.to_string())?
    } else {
        state.store.remove_job(job_id).await.map_err(|e| e.to_string())?
    };
    if !removed {
        return Err(format!("iş {job_id} çalışıyor veya yok"));
    }

    // Without the output-folder root, jobs under a custom `out_dir` would leave
    // their folders behind.
    let out_dir = PathBuf::from(&job.out_dir);
    let canonical = out_dir.canonicalize().unwrap_or(out_dir.clone());
    let roots = [
        data_dir().canonicalize().unwrap_or_else(|_| data_dir()),
        absolute(&UiSettings::load().out_dir),
    ];
    if canonical.is_dir() && roots.iter().any(|r| canonical != *r && canonical.starts_with(r)) {
        std::fs::remove_dir_all(&canonical).map_err(|e| e.to_string())?;
    }
    let _ = app.emit(
        "project-deleted",
        serde_json::json!({ "job_id": job_id }),
    );
    Ok(())
}

/// Deletes one translation. The source transcript can't be targeted: it
/// would need a new STT run to come back.
#[tauri::command]
async fn remove_translation(
    state: State<'_, AppState>,
    job_id: i64,
    lang: String,
) -> Result<(), String> {
    let canonical = subtitler_core::lang::by_code(&lang)
        .map(|l| l.code.to_string())
        .ok_or_else(|| format!("bilinmeyen dil kodu: {lang}"))?;
    if state.translating.lock().map_err(|_| "lock poisoned".to_string())?.contains(&job_id) {
        return Err("bu iş şu anda çevriliyor".into());
    }
    let paths = state
        .store
        .remove_translation(job_id, &[canonical.clone()])
        .await
        .map_err(|e| e.to_string())?;
    for path in paths {
        // Belt and braces: never delete `source.*`.
        let is_source = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("source."));
        if is_source {
            continue;
        }
        let _ = std::fs::remove_file(&path);
    }
    let _ = state.events.send(JobEvent::TranslationRemoved {
        job: JobId(job_id as u64),
        langs: vec![canonical],
    });
    Ok(())
}

/// Adds target languages to a finished job. `run_file` reuses `words.json`
/// and the source track, so only the new languages cost LLM calls.
#[tauri::command]
async fn translate_job(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: i64,
    targets: Vec<String>,
) -> Result<(), String> {
    let targets: Vec<String> =
        targets.into_iter().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect();
    if targets.is_empty() {
        return Err("no target languages given".into());
    }

    let job = state
        .store
        .job_by_id(job_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("job {job_id} not found"))?;
    if job.status != "done" {
        return Err("the job must be completed before it can be translated".into());
    }
    if !state.translating.lock().map_err(|_| "lock poisoned".to_string())?.insert(job_id) {
        return Err("a translation is already running for this job".into());
    }

    let settings = UiSettings::load();
    let cfg = RunConfig {
        langs: targets.clone(),
        ..settings.to_run_config()
    };
    let llm = settings.build_llm()?.ok_or("no LLM configured")?;

    // URL jobs translate from the downloaded file.
    let input = state
        .store
        .list_artifacts(job_id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|a| a.kind == "source")
        .map(|a| PathBuf::from(a.path))
        .unwrap_or_else(|| PathBuf::from(&job.source_ref));
    if !input.is_file() {
        state
            .translating
            .lock()
            .map_err(|_| "lock poisoned".to_string())?
            .remove(&job_id);
        return Err(format!("source video not found: {}", input.display()));
    }

    let store = state.store.clone();
    let events = state.events.clone();
    let translating = state.translating.clone();
    let reporter = StageReporter::for_job(store.clone(), JobId(job_id as u64), events.clone());
    let out_dir = PathBuf::from(&job.out_dir);
    // Must match the pipeline's layout (`paths::JobPaths`).
    let paths = subtitler_pipeline::paths::JobPaths::new(&out_dir);
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        let result = subtitler_pipeline::run_file(
            &input,
            &out_dir,
            &NeverStt as &dyn subtitler_stt::SttProvider,
            Some(llm.as_ref()),
            &cfg,
            Some(&reporter),
        )
        .await;

        if let Ok(mut busy) = translating.lock() {
            busy.remove(&job_id);
        }

        match result {
            Ok(_artifacts) => {
                for target in &targets {
                    let _ = store
                        .add_artifact(job_id, "srt", target, &paths.srt(target))
                        .await;
                    let _ = store
                        .add_artifact(job_id, "track", target, &paths.translation_track(target))
                        .await;
                }
                let _ = events.send(JobEvent::JobDone {
                    job: JobId(job_id as u64),
                    outputs: targets.clone(),
                });
                let _ = app_handle.emit(
                    "translation-done",
                    serde_json::json!({ "job_id": job_id, "langs": targets }),
                );
            }
            Err(e) => {
                let _ = events.send(JobEvent::Warning {
                    job: JobId(job_id as u64),
                    message: format!("translation failed: {e}"),
                });
                let _ = app_handle.emit(
                    "translation-failed",
                    serde_json::json!({ "job_id": job_id, "error": e.to_string() }),
                );
            }
        }
    });
    Ok(())
}

/// `run_file` never reaches STT when `words.json` exists, so any call here is
/// a bug.
struct NeverStt;

#[async_trait::async_trait]
impl subtitler_stt::SttProvider for NeverStt {
    fn name(&self) -> &'static str {
        "none"
    }

    async fn transcribe_chunk(
        &self,
        _chunk: &subtitler_stt::AudioChunk,
        _opts: &subtitler_stt::TranscribeOptions,
    ) -> Result<subtitler_stt::Transcription, subtitler_stt::SttError> {
        Err(subtitler_stt::SttError::BadResponse {
            provider: "none",
            message: "translation-only run reached STT (words.json missing?)".into(),
        })
    }
}

#[tauri::command]
async fn list_jobs(
    state: State<'_, AppState>,
    batch: Option<i64>,
) -> Result<serde_json::Value, String> {
    let jobs = state.store.list_jobs(batch).await.map_err(|e| e.to_string())?;
    serde_json::to_value(jobs).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_artifacts(state: State<'_, AppState>, job_id: i64) -> Result<serde_json::Value, String> {
    let artifacts = state.store.list_artifacts(job_id).await.map_err(|e| e.to_string())?;
    serde_json::to_value(artifacts).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_artifact(path: String) -> Result<serde_json::Value, String> {
    if !path.ends_with(".json") {
        return Err("only .json artifacts can be read".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

/// Key availability and source ("file", "env" or ""). Values never leave
/// the backend.
#[tauri::command]
fn keys_status() -> serde_json::Value {
    serde_json::json!({
        "GROQ_API_KEY": resolve_key(ProviderKind::Groq).is_some(),
        "OPENROUTER_API_KEY": resolve_key(ProviderKind::OpenRouter).is_some(),
        "sources": {
            "GROQ_API_KEY": key_source(ProviderKind::Groq),
            "OPENROUTER_API_KEY": key_source(ProviderKind::OpenRouter),
        },
    })
}

/// Stored keys only; environment values are never echoed back.
#[tauri::command]
fn load_keys() -> serde_json::Value {
    let store = KeyStore::load();
    serde_json::json!({
        "groq": store.groq,
        "openrouter": store.openrouter,
    })
}

/// Patch semantics: a string replaces, empty or null clears, absent leaves
/// it alone.
#[tauri::command]
fn save_keys(patch: serde_json::Value) -> Result<(), String> {
    let mut store = KeyStore::load();
    for (field, kind) in [
        ("groq", ProviderKind::Groq),
        ("openrouter", ProviderKind::OpenRouter),
    ] {
        if let Some(value) = patch.get(field) {
            store.set(kind, value.as_str());
        }
    }
    store.save()
}

#[tauri::command]
async fn model_catalog(refresh: Option<bool>) -> Result<serde_json::Value, String> {
    Ok(catalog::get_catalogue(refresh.unwrap_or(false)).await)
}

#[tauri::command]
fn language_options() -> Vec<Language> {
    lang::LANGUAGES.to_vec()
}

#[tauri::command]
fn keys_path() -> String {
    KeyStore::path().display().to_string()
}

#[tauri::command]
/// Opens a folder (not `open -R`, which only reveals). Restricted to the
/// `data/` tree.
fn open_folder(path: String) -> Result<(), String> {
    let dir = absolute(&path);
    if !dir.is_dir() {
        return Err(format!("klasör bulunamadı: {path}"));
    }
    let root = data_dir();
    let (target, base) = match (dir.canonicalize(), root.canonicalize()) {
        (Ok(t), Ok(b)) => (t, b),
        _ => return Err("klasör çözümlenemedi".into()),
    };
    if !target.starts_with(&base) {
        return Err("yalnızca uygulama klasörleri açılabilir".into());
    }
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&target).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("explorer").arg(&target).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(&target).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    let target = absolute(&path);
    if !target.exists() {
        return Err(format!("yol bulunamadı: {path}"));
    }
    let root = data_dir();
    let (t, base) = match (target.canonicalize(), root.canonicalize()) {
        (Ok(t), Ok(b)) => (t, b),
        _ => return Err("yol çözümlenemedi".into()),
    };
    if !t.starts_with(&base) {
        return Err("yalnızca uygulama klasörleri açılabilir".into());
    }
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&t)
            .spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", t.display()))
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(t.parent().unwrap_or(&t)).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}


pub fn run() {
    let root = repo_root();
    std::env::set_current_dir(&root).ok();
    let store_path = data_dir().join("store.db");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let store =
                tauri::async_runtime::block_on(Store::open(&store_path)).map_err(|e| e.to_string())?;

            // Best-effort cleanup of orphaned output folders; must never block startup.
            {
                let out_root = absolute(&UiSettings::load().out_dir);
                let job_dirs: Vec<std::path::PathBuf> =
                    tauri::async_runtime::block_on(store.list_jobs(None))
                        .unwrap_or_default()
                        .into_iter()
                        .map(|j| std::path::PathBuf::from(&j.out_dir))
                        .collect();
                gc::collect(&out_root, &job_dirs);
            }
            let events = broadcast::channel(512).0;
            let handle = app.handle().clone();
            let mut rx = events.subscribe();
            tauri::async_runtime::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    let _ = handle.emit("job-event", serde_json::to_value(event).unwrap_or_default());
                }
            });
            app.manage(AppState {
                store: Arc::new(store),
                queue_running: Mutex::new(false),
                events,
                translating: Arc::new(Mutex::new(HashSet::new())),
            });

            // The window starts hidden and is shown on `frontend-ready`, with a 10 s
            // fallback so it can't stay invisible.
            let shown = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let handle = app.handle().clone();
            let handle_for_listener = handle.clone();
            let shown_for_listener = shown.clone();
            handle.listen_any("frontend-ready", move |_event| {
                if shown_for_listener.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                if let Some(w) = handle_for_listener.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            });
            let handle_for_timeout = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                if !shown.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Some(w) = handle_for_timeout.get_webview_window("main") {
                        let _ = w.show();
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            settings_path,
            open_folder,
            create_batch,
            start_queue,
            cancel_queue,
            retry_job,
            remove_job,
            translate_job,
            list_jobs,
            list_artifacts,
            read_artifact,
            reveal_path,
            keys_status,
            load_keys,
            save_keys,
            keys_path,
            model_catalog,
            language_options,
            remove_translation,
            retranscribe_job,
            export_formats,
            export_tracks,
            delete_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs against a throwaway `SUBTITLER_ROOT`, never the real `data/`.
    #[test]
    fn key_store_roundtrip_permissions_and_precedence() {
        let root = std::env::temp_dir().join(format!("sub0-keys-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp repo root");
        std::env::set_var("SUBTITLER_ROOT", &root);
        std::env::set_var("GROQ_API_KEY", "env-groq");

        let mut store = KeyStore::default();
        store.set(ProviderKind::Groq, Some("  file-groq  "));
        store.set(ProviderKind::OpenRouter, Some("file-or"));
        store.save().expect("save");

        assert_eq!(resolve_key(ProviderKind::Groq).as_deref(), Some("file-groq"));
        assert_eq!(key_source(ProviderKind::Groq), "file");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(KeyStore::path()).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "keys.json must stay owner-only");
        }

        save_keys(serde_json::json!({ "openrouter": "or-1" })).expect("save_keys");
        assert_eq!(KeyStore::load().get(ProviderKind::OpenRouter), Some("or-1"));
        save_keys(serde_json::json!({})).expect("empty patch");
        assert_eq!(
            KeyStore::load().get(ProviderKind::OpenRouter),
            Some("or-1"),
            "an absent field must not clear the entry"
        );
        save_keys(serde_json::json!({ "openrouter": null })).expect("clear");
        assert!(KeyStore::load().get(ProviderKind::OpenRouter).is_none());

        let mut store = KeyStore::load();
        store.set(ProviderKind::Groq, Some("   "));
        store.save().expect("save");
        assert!(KeyStore::load().get(ProviderKind::Groq).is_none());
        let raw = std::fs::read_to_string(KeyStore::path()).unwrap();
        assert!(!raw.contains("\"groq\": \"\""), "empty secret must not be stored");

        assert_eq!(resolve_key(ProviderKind::Groq).as_deref(), Some("env-groq"));
        assert_eq!(key_source(ProviderKind::Groq), "env");

        std::env::remove_var("OPENROUTER_API_KEY");
        assert!(resolve_key(ProviderKind::OpenRouter).is_none());
        assert_eq!(key_source(ProviderKind::OpenRouter), "");

        let status = keys_status();
        assert_eq!(status["GROQ_API_KEY"], serde_json::json!(true));
        assert_eq!(status["sources"]["GROQ_API_KEY"], serde_json::json!("env"));
        assert_eq!(status["OPENROUTER_API_KEY"], serde_json::json!(false));
        assert!(!status.to_string().contains("env-groq"), "status must not echo secrets");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A stale Groq slug used to reach OpenRouter and fail with 400.
    #[test]
    fn repair_stt_model_remaps_a_slug_from_the_other_provider() {
        let mut s = UiSettings {
            stt_provider: "openrouter".into(),
            stt_model: "whisper-large-v3".into(),
            ..UiSettings::default()
        };
        s.repair_stt_model();
        assert_eq!(s.stt_model, "openai/whisper-large-v3");

        let mut s = UiSettings {
            stt_provider: "groq".into(),
            stt_model: "openai/whisper-large-v3".into(),
            ..UiSettings::default()
        };
        s.repair_stt_model();
        assert_eq!(s.stt_model, "whisper-large-v3");
    }

    #[test]
    fn repair_stt_model_keeps_a_valid_or_custom_slug() {
        for (provider, model) in [
            ("openrouter", "openai/whisper-large-v3-turbo"),
            ("openrouter", "microsoft/mai-transcribe-1"),
            ("groq", "whisper-large-v3-turbo"),
        ] {
            let mut s = UiSettings {
                stt_provider: provider.into(),
                stt_model: model.into(),
                ..UiSettings::default()
            };
            s.repair_stt_model();
            assert_eq!(s.stt_model, model, "{provider} must keep {model}");
        }
    }

    #[test]
    fn default_settings_pair_provider_with_its_own_slug() {
        let s = UiSettings::default();
        assert_eq!(s.stt_provider, "groq");
        assert!(
            SttSettings::is_model_valid_for(ProviderKind::Groq, &s.stt_model),
            "default stt_model {:?} must match the default provider",
            s.stt_model
        );
    }

    #[test]
    fn repair_langs_keeps_a_valid_selection_in_order() {
        let mut s = UiSettings {
            langs: vec!["tr".into(), "de".into(), "zh-Hans".into()],
            ..UiSettings::default()
        };
        s.repair_langs();
        assert_eq!(s.langs, vec!["tr", "de", "zh-Hans"]);
    }

    #[test]
    fn repair_langs_drops_unknown_codes_and_duplicates() {
        let mut s = UiSettings {
            langs: vec!["DE".into(), "de".into(), "klingon".into(), "tr".into()],
            ..UiSettings::default()
        };
        s.repair_langs();
        assert_eq!(s.langs, vec!["de", "tr"]);
    }

    #[test]
    fn repair_langs_rejects_unknown_and_path_shaped_codes() {
        for langs in [
            // A hand-edited settings.toml must not smuggle a path into `{target}.srt`.
            vec!["../../etc/passwd".to_string()],
            vec!["en/../x".to_string()],
            vec!["klingon".to_string(), "tr".to_string()],
        ] {
            let mut s = UiSettings {
                langs,
                ..UiSettings::default()
            };
            s.repair_langs();
            assert!(
                s.langs.iter().all(|c| lang::is_supported(c)),
                "geçersiz kod kalmamalı: {:?}",
                s.langs
            );
        }
    }

    /// An empty language list is a deliberate "transcript only".
    #[test]
    fn transcript_only_survives_a_repair() {
        let mut s = UiSettings {
            translate: false,
            langs: Vec::new(),
            ..UiSettings::default()
        };
        s.repair_langs();
        assert!(s.langs.is_empty(), "transcript-only bozulmamalı");
        assert!(!s.translate, "dil yoksa çeviri kapalı olmalı");
    }

    #[test]
    fn a_fully_invalid_selection_turns_translation_off() {
        let mut s = UiSettings {
            translate: true,
            langs: vec!["klingon".to_string()],
            ..UiSettings::default()
        };
        s.repair_langs();
        assert!(s.langs.is_empty());
        assert!(!s.translate);
    }

    #[test]
    fn language_options_expose_every_selectable_target() {
        let options = language_options();
        assert_eq!(options.len(), lang::LANGUAGES.len());
        assert!(options.len() > 150, "the picker needs the full list");
        for expected in ["tr", "de", "en", "zh-Hans", "pt-BR"] {
            assert!(
                options.iter().any(|l| l.code == expected),
                "{expected} must be offered to the picker"
            );
        }
    }

    #[test]
    fn export_stem_comes_from_the_record_not_the_caller() {
        let stem_for = |file: &str| -> String {
            match std::path::Path::new(file)
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|f| f.strip_suffix(".track.json"))
            {
                Some("source") | None => "kaynak".to_string(),
                Some(lang) => lang.to_string(),
            }
        };
        assert_eq!(stem_for("out/source.track.json"), "kaynak");
        assert_eq!(stem_for("out/tr.track.json"), "tr");
        assert_eq!(stem_for("out/zh-Hans.track.json"), "zh-Hans");
        assert!(!stem_for("../../etc/passwd").contains('/'));
    }

    #[test]
    fn export_formats_cover_the_supported_carriers() {
        let formats = export_formats();
        assert_eq!(formats.len(), 3, "srt, vtt, txt");
        for f in ["srt", "vtt", "txt"] {
            assert!(formats.contains(&f), "{f} desteklenmeli");
            assert_eq!(
                subtitler_export::Format::parse(f).map(|x| x.as_str()),
                Some(f),
                "biçim adı uzantıyla aynı olmalı"
            );
        }
    }

    #[test]
    fn folder_commands_refuse_paths_outside_the_app_data() {
        // No data_dir exists here, so the error proves the guard ran first.
        let outside = std::env::temp_dir();
        let err = open_folder(outside.to_string_lossy().to_string()).unwrap_err();
        assert!(!err.is_empty(), "dış yol reddedilmeli");
        let err = reveal_path(outside.join("yok.srt").to_string_lossy().to_string()).unwrap_err();
        assert!(err.contains("bulunamadı"), "olmayan dosya hatası beklenir: {err}");
    }
}
