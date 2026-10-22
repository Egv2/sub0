//! Persistent, sequential job queue with crash-safe resume and download prefetch.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use subtitler_core::job::{JobEvent, JobId, JobStage};
use subtitler_store::Store;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use subtitler_media as media;

use crate::paths::JobPaths;
use crate::{run_file, PipelineError, RunConfig, StageReporter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct QueueSummary {
    pub done: u64,
    pub failed: u64,
    pub cancelled: bool,
}

pub struct QueueRun {
    pub store: Arc<Store>,
    pub stt: Box<dyn subtitler_stt::SttProvider>,
    pub llm: Option<Box<dyn subtitler_translate::LlmProvider>>,
    pub cfg: RunConfig,
    pub events: broadcast::Sender<JobEvent>,
    /// `None` drains every batch; `Some(id)` drains one.
    pub batch: Option<i64>,
    pub limit: Option<u64>,
    /// How many upcoming URL jobs to download while the current one runs. `0` disables it.
    pub prefetch_ahead: u32,
    pub cancel: CancellationToken,
}

/// Drain the queue one job at a time.
///
/// Prefetching is purely a speedup: a job awaits its own prefetch task (so the
/// inline download can never race it) and falls back to an inline download if
/// the prefetch failed, keeping retry/failure behaviour identical.
pub async fn run_queue(run: QueueRun) -> Result<QueueSummary, PipelineError> {
    let QueueRun {
        store,
        stt,
        llm,
        cfg,
        events,
        batch,
        limit,
        prefetch_ahead,
        cancel,
    } = run;

    let ytdlp_version = media::ytdlp::version().await.map_err(|e| match e {
        media::MediaError::BinaryNotFound(_) => PipelineError::YtdlpMissing,
        other => PipelineError::Media(other),
    })?;
    println!("yt-dlp {ytdlp_version}");

    // Anything left `running` was interrupted; existing artifacts make the rerun cheap.
    let reset = store.reset_stale_running().await?;
    if reset > 0 {
        println!("resume: {reset} interrupted job(s) re-queued");
    }

    let mut prefetches: HashMap<i64, tokio::task::JoinHandle<Option<PathBuf>>> = HashMap::new();

    let (mut done, mut failed, mut processed) = (0u64, 0u64, 0u64);
    loop {
        if cancel.is_cancelled() || limit.is_some_and(|l| processed >= l) {
            break;
        }
        let Some(job) = store.next_queued_job(batch).await? else {
            break;
        };
        store.claim_job(job.id).await?;
        let job_id = JobId(job.id as u64);
        let reporter = StageReporter::for_job(store.clone(), job_id, events.clone());
        let out_dir = PathBuf::from(&job.out_dir);
        println!("[job {}] {} ({})", job.id, job.source_ref, job.source_kind);

        // The current job is claimed, so everything still `queued` is upcoming.
        spawn_prefetches(
            &store,
            batch,
            prefetch_ahead as usize,
            &cancel,
            &mut prefetches,
        )
        .await;

        let input: PathBuf = if job.source_kind == "url" {
            reporter.report(JobStage::Downloading).await;
            let video = match match prefetches.remove(&job.id) {
                Some(handle) => handle.await.ok().flatten(),
                None => None,
            } {
                Some(path) => Ok(subtitler_media::DownloadedVideo {
                    path,
                    final_percent: None,
                }),
                None => download_with_retry(&job.source_ref, &out_dir, &cancel).await,
            };
            match video {
                Ok(video) => {
                    let _ = store.add_artifact(job.id, "source", "", &video.path).await;
                    video.path
                }
                Err(e) => {
                    let class = media::failure_class(&e);
                    let _ = store
                        .mark_job_failed(job.id, class_name(class), &e.to_string())
                        .await;
                    let _ = events.send(JobEvent::JobFailed {
                        job: job_id,
                        error: e.to_string(),
                        retryable: class == media::FailureClass::Transient,
                    });
                    eprintln!("[job {}] download failed ({class:?}): {e}", job.id);
                    failed += 1;
                    processed += 1;
                    continue; // one failing job never stops the queue
                }
            }
        } else {
            PathBuf::from(&job.source_ref)
        };

        // Dropping the future on cancel kills ffmpeg/yt-dlp children via kill_on_drop.
        let result = tokio::select! {
            r = run_file(&input, &out_dir, stt.as_ref(), llm.as_deref(), &cfg, Some(&reporter)) => r,
            _ = cancel.cancelled() => {
                // Left `running` on purpose; the next run requeues and resumes it.
                let _ = events.send(JobEvent::Warning {
                    job: job_id,
                    message: "cancelled — will resume on next queue run".into(),
                });
                abort_prefetches(&mut prefetches);
                return Ok(QueueSummary { done, failed, cancelled: true });
            }
        };

        match result {
            Ok(artifacts) => {
                // Artifacts live under the hidden `.sub0/`; always go through JobPaths.
                let paths = JobPaths::new(&out_dir);
                let _ = store.add_artifact(job.id, "words", "", &paths.words()).await;
                let _ = store
                    .add_artifact(job.id, "track", "", &paths.source_track())
                    .await;
                for (lang, path) in &artifacts.srt_paths {
                    let _ = store.add_artifact(job.id, "srt", lang, path).await;
                }
                // The UI only lists translations that are registered here.
                for (lang, path) in &artifacts.translation_paths {
                    let _ = store.add_artifact(job.id, "track", lang, path).await;
                }
                store.mark_job_done(job.id).await?;
                let _ = events.send(JobEvent::JobDone {
                    job: job_id,
                    outputs: artifacts
                        .srt_paths
                        .iter()
                        .map(|(_, p)| p.display().to_string())
                        .collect(),
                });
                done += 1;
            }
            Err(e) => {
                let class = e.failure_class();
                let _ = store
                    .mark_job_failed(job.id, class_name(class), &e.to_string())
                    .await;
                let _ = events.send(JobEvent::JobFailed {
                    job: job_id,
                    error: e.to_string(),
                    retryable: class == media::FailureClass::Transient,
                });
                eprintln!("[job {}] failed ({class:?}): {e}", job.id);
                failed += 1;
            }
        }
        processed += 1;
    }
    abort_prefetches(&mut prefetches);

    Ok(QueueSummary {
        done,
        failed,
        cancelled: false,
    })
}

async fn spawn_prefetches(
    store: &Store,
    batch: Option<i64>,
    ahead: usize,
    cancel: &CancellationToken,
    prefetches: &mut HashMap<i64, tokio::task::JoinHandle<Option<PathBuf>>>,
) {
    if ahead == 0 || cancel.is_cancelled() {
        return;
    }
    let Ok(jobs) = store.list_jobs(batch).await else {
        return; // prefetching is best-effort; the inline download covers us
    };
    for upcoming in prefetch_candidates(&jobs, ahead) {
        if prefetches.contains_key(&upcoming.id) {
            continue;
        }
        let url = upcoming.source_ref.clone();
        let out_dir = PathBuf::from(&upcoming.out_dir);
        let task_cancel = cancel.clone();
        println!("[prefetch {}] {url}", upcoming.id);
        prefetches.insert(
            upcoming.id,
            tokio::spawn(async move {
                if task_cancel.is_cancelled() {
                    return None;
                }
                // On failure the job just downloads inline on its own turn.
                download_with_retry(&url, &out_dir, &task_cancel)
                    .await
                    .ok()
                    .map(|video| video.path)
            }),
        );
    }
}

fn prefetch_candidates(
    jobs: &[subtitler_store::JobRow],
    ahead: usize,
) -> Vec<subtitler_store::JobRow> {
    jobs.iter()
        .filter(|j| j.status == "queued" && j.source_kind == "url")
        .take(ahead)
        .cloned()
        .collect()
}

// `kill_on_drop` reaps the yt-dlp children.
fn abort_prefetches(prefetches: &mut HashMap<i64, tokio::task::JoinHandle<Option<PathBuf>>>) {
    for (_, handle) in prefetches.drain() {
        handle.abort();
    }
}

async fn download_with_retry(
    url: &str,
    out_dir: &std::path::Path,
    cancel: &CancellationToken,
) -> Result<subtitler_media::DownloadedVideo, subtitler_media::MediaError> {
    let mut attempt = 0u32;
    loop {
        let result = tokio::select! {
            r = subtitler_media::download(url, out_dir, true) => r,
            _ = cancel.cancelled() => {
                return Err(subtitler_media::MediaError::Parse(
                    "yt-dlp".into(),
                    "cancelled".into(),
                ));
            }
        };
        match result {
            Ok(video) => return Ok(video),
            Err(e) if media::failure_class(&e) == media::FailureClass::Transient && attempt < 2 => {
                let wait = 2 * (attempt + 1);
                eprintln!("[download] transient failure ({e}); retrying in {wait} s");
                tokio::time::sleep(std::time::Duration::from_secs(wait as u64)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

fn class_name(class: media::FailureClass) -> &'static str {
    match class {
        media::FailureClass::Transient => "transient",
        media::FailureClass::Permanent => "permanent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use subtitler_store::JobRow;

    fn job(id: i64, kind: &str, status: &str) -> JobRow {
        JobRow {
            id,
            batch_id: 1,
            source_kind: kind.into(),
            source_ref: format!("ref-{id}"),
            stage: "queued".into(),
            status: status.into(),
            error_class: None,
            error_msg: None,
            attempts: 0,
            out_dir: format!("o/{id}"),
        }
    }

    #[test]
    fn prefetch_picks_queued_url_jobs_bounded_by_lookahead() {
        let jobs = vec![
            job(1, "url", "running"), // the claimed current job — excluded
            job(2, "file", "queued"), // file jobs are never prefetched
            job(3, "url", "queued"),
            job(4, "url", "queued"),
            job(5, "url", "queued"),
            job(6, "url", "failed"),
            job(7, "url", "done"),
        ];
        let ids = |ahead: usize| -> Vec<i64> {
            prefetch_candidates(&jobs, ahead)
                .into_iter()
                .map(|j| j.id)
                .collect()
        };
        assert_eq!(ids(2), vec![3, 4]);
        assert_eq!(ids(10), vec![3, 4, 5]);
        assert!(ids(0).is_empty());
        assert!(prefetch_candidates(&[], 2).is_empty());
    }
}
