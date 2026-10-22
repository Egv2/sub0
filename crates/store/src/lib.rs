//! Embedded SQLite store: LLM/STT caches, usage tracking and the job queue.
//! Cache keys hash the prompt too, so editing a prompt invalidates only what it affects.

use std::path::Path;

use sqlx::{sqlite::SqlitePool, Row};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store at `{0}` failed: {1}")]
    Sqlite(String, #[source] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmCacheEntry {
    pub response: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
    path: String,
}

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS llm_cache (\
         key TEXT PRIMARY KEY,\
         response_json TEXT NOT NULL,\
         tokens_in INTEGER NOT NULL DEFAULT 0,\
         tokens_out INTEGER NOT NULL DEFAULT 0,\
         created_at TEXT NOT NULL DEFAULT (datetime('now')))",
    "CREATE TABLE IF NOT EXISTS stt_cache (\
         key TEXT PRIMARY KEY,\
         response_json TEXT NOT NULL,\
         created_at TEXT NOT NULL DEFAULT (datetime('now')))",
    "CREATE TABLE IF NOT EXISTS usage (\
         id INTEGER PRIMARY KEY AUTOINCREMENT,\
         stage TEXT NOT NULL,\
         provider TEXT NOT NULL,\
         model TEXT NOT NULL,\
         tokens_in INTEGER NOT NULL DEFAULT 0,\
         tokens_out INTEGER NOT NULL DEFAULT 0,\
         usd REAL,\
         created_at TEXT NOT NULL DEFAULT (datetime('now')))",
    "CREATE TABLE IF NOT EXISTS batches (\
         id INTEGER PRIMARY KEY AUTOINCREMENT,\
         settings_json TEXT NOT NULL DEFAULT '{}',\
         created_at TEXT NOT NULL DEFAULT (datetime('now')))",
    "CREATE TABLE IF NOT EXISTS jobs (\
         id INTEGER PRIMARY KEY AUTOINCREMENT,\
         batch_id INTEGER NOT NULL REFERENCES batches(id),\
         source_kind TEXT NOT NULL CHECK (source_kind IN ('file', 'url')),\
         source_ref TEXT NOT NULL,\
         stage TEXT NOT NULL DEFAULT 'queued',\
         status TEXT NOT NULL DEFAULT 'queued'\
             CHECK (status IN ('queued', 'running', 'done', 'failed', 'cancelled')),\
         error_class TEXT,\
         error_msg TEXT,\
         attempts INTEGER NOT NULL DEFAULT 0,\
         out_dir TEXT NOT NULL,\
         created_at TEXT NOT NULL DEFAULT (datetime('now')),\
         updated_at TEXT NOT NULL DEFAULT (datetime('now')))",
    "CREATE TABLE IF NOT EXISTS artifacts (\
         job_id INTEGER NOT NULL REFERENCES jobs(id),\
         kind TEXT NOT NULL,\
         path TEXT NOT NULL,\
         lang TEXT NOT NULL DEFAULT '',\
         created_at TEXT NOT NULL DEFAULT (datetime('now')),\
         PRIMARY KEY (job_id, kind, lang))",
];

// `{lang}.track.json` / `{lang}.srt` -> lang; the source and other kinds -> "".
fn artifact_lang(kind: &str, path: &str) -> String {
    if !matches!(kind, "track" | "srt") {
        return String::new();
    }
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file
        .strip_suffix(".track.json")
        .or_else(|| file.strip_suffix(".srt"))
        .unwrap_or(file);
    if stem == "source" {
        return String::new();
    }
    stem.to_string()
}

// The old `(job_id, kind)` key let the last translation overwrite the source
// transcript's row. Re-key on `(job_id, kind, lang)` and recover the source
// row from whatever is actually on disk.
async fn migrate_artifacts(pool: &sqlx::SqlitePool, db: &str) -> Result<(), StoreError> {
    let err = |e: sqlx::Error| StoreError::Sqlite(db.to_string(), e);
    let columns = sqlx::query("PRAGMA table_info(artifacts)")
        .fetch_all(pool)
        .await
        .map_err(err)?;
    if columns
        .iter()
        .any(|c| c.try_get::<String, _>("name").ok().as_deref() == Some("lang"))
    {
        return Ok(());
    }

    let rows: Vec<(i64, String, String)> = sqlx::query("SELECT job_id, kind, path FROM artifacts")
        .fetch_all(pool)
        .await
        .map_err(err)?
        .iter()
        .map(|r| {
            (
                r.get::<i64, _>("job_id"),
                r.get::<String, _>("kind"),
                r.get::<String, _>("path"),
            )
        })
        .collect();

    // SQLite can refuse the DROP while foreign keys are enforced.
    let _ = sqlx::query("PRAGMA foreign_keys = OFF").execute(pool).await;
    sqlx::query("DROP TABLE IF EXISTS artifacts").execute(pool).await.map_err(err)?;
    sqlx::query(
        "CREATE TABLE artifacts (\
             job_id INTEGER NOT NULL REFERENCES jobs(id),\
             kind TEXT NOT NULL,\
             path TEXT NOT NULL,\
             lang TEXT NOT NULL DEFAULT '',\
             created_at TEXT NOT NULL DEFAULT (datetime('now')),\
             PRIMARY KEY (job_id, kind, lang))",
    )
    .execute(pool)
    .await
    .map_err(err)?;

    for (job_id, kind, path) in rows {
        sqlx::query(
            "INSERT OR REPLACE INTO artifacts (job_id, kind, path, lang) \
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(job_id)
        .bind(&kind)
        .bind(&path)
        .bind(artifact_lang(&kind, &path))
        .execute(pool)
        .await
        .map_err(err)?;
    }

    let jobs = sqlx::query("SELECT id, out_dir FROM jobs")
        .fetch_all(pool)
        .await
        .map_err(err)?;
    for row in jobs {
        let job_id = row.get::<i64, _>("id");
        let out_dir = row.get::<String, _>("out_dir");
        let source = std::path::PathBuf::from(&out_dir).join("source.track.json");
        if !source.is_file() {
            continue;
        }
        sqlx::query(
            "INSERT INTO artifacts (job_id, kind, path, lang) VALUES (?1, 'track', ?2, '') \
             ON CONFLICT(job_id, kind, lang) DO UPDATE SET path = excluded.path",
        )
        .bind(job_id)
        .bind(source.to_string_lossy().as_ref())
        .execute(pool)
        .await
        .map_err(err)?;
    }
    let _ = sqlx::query("PRAGMA foreign_keys = ON").execute(pool).await;
    Ok(())
}

impl Store {
    pub async fn open(path: &Path) -> Result<Store, StoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let path_str = path.to_string_lossy().into_owned();
        let url = format!("sqlite://{path_str}?mode=rwc");
        let pool = SqlitePool::connect(&url)
            .await
            .map_err(|e| StoreError::Sqlite(path_str.clone(), e))?;
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&pool)
            .await
            .map_err(|e| StoreError::Sqlite(path_str.clone(), e))?;
        for stmt in SCHEMA {
            sqlx::query(stmt)
                .execute(&pool)
                .await
                .map_err(|e| StoreError::Sqlite(path_str.clone(), e))?;
        }
        migrate_artifacts(&pool, &path_str).await?;

        // Older versions left finished jobs on their last stage (e.g. "Uploading"),
        // which the UI read as still running.
        sqlx::query("UPDATE jobs SET stage = 'done' WHERE status IN ('done', 'failed', 'cancelled') \
                     AND stage NOT IN ('done', 'failed', 'cancelled')")
            .execute(&pool)
            .await
            .map_err(|e| StoreError::Sqlite(path_str.clone(), e))?;
        Ok(Store {
            pool,
            path: path_str,
        })
    }

    pub fn hash_hex(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_string()
    }

    /// Parts are NUL-separated so "ab"+"c" and "a"+"bc" don't collide.
    pub fn cache_key(parts: &[&str]) -> String {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            hasher.update(part.as_bytes());
            hasher.update(&[0]);
        }
        hasher.finalize().to_string()
    }

    pub async fn llm_get(&self, key: &str) -> Result<Option<LlmCacheEntry>, StoreError> {
        let row = sqlx::query(
            "SELECT response_json, tokens_in, tokens_out FROM llm_cache WHERE key = ?1",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(row.map(|r| LlmCacheEntry {
            response: r.get("response_json"),
            tokens_in: r.get::<i64, _>("tokens_in") as u64,
            tokens_out: r.get::<i64, _>("tokens_out") as u64,
        }))
    }

    pub async fn llm_put(
        &self,
        key: &str,
        response: &str,
        tokens_in: u64,
        tokens_out: u64,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO llm_cache (key, response_json, tokens_in, tokens_out) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(key) DO UPDATE SET response_json = excluded.response_json, \
             tokens_in = excluded.tokens_in, tokens_out = excluded.tokens_out",
        )
        .bind(key)
        .bind(response)
        .bind(tokens_in as i64)
        .bind(tokens_out as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(())
    }

    pub async fn stt_get(&self, key: &str) -> Result<Option<String>, StoreError> {
        let row = sqlx::query("SELECT response_json FROM stt_cache WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(row.map(|r| r.get("response_json")))
    }

    pub async fn stt_put(&self, key: &str, response: &str) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO stt_cache (key, response_json) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET response_json = excluded.response_json",
        )
        .bind(key)
        .bind(response)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(())
    }

    pub async fn usage(
        &self,
        stage: &str,
        provider: &str,
        model: &str,
        tokens_in: u64,
        tokens_out: u64,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO usage (stage, provider, model, tokens_in, tokens_out) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(stage)
        .bind(provider)
        .bind(model)
        .bind(tokens_in as i64)
        .bind(tokens_out as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(())
    }

    // Settings live on the batch only; jobs look them up rather than copying them.
    pub async fn batch_settings(&self, batch_id: i64) -> Result<Option<String>, StoreError> {
        let row = sqlx::query_scalar("SELECT settings_json FROM batches WHERE id = ?1")
            .bind(batch_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(row)
    }

    pub async fn create_batch(&self, settings_json: &str) -> Result<i64, StoreError> {
        let row = sqlx::query("INSERT INTO batches (settings_json) VALUES (?1) RETURNING id")
            .bind(settings_json)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(row.get("id"))
    }

    pub async fn enqueue_job(
        &self,
        batch_id: i64,
        source_kind: &str,
        source_ref: &str,
        out_dir: &Path,
    ) -> Result<i64, StoreError> {
        let row = sqlx::query(
            "INSERT INTO jobs (batch_id, source_kind, source_ref, out_dir) \
             VALUES (?1, ?2, ?3, ?4) RETURNING id",
        )
        .bind(batch_id)
        .bind(source_kind)
        .bind(source_ref)
        .bind(out_dir.to_string_lossy().as_ref())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(row.get("id"))
    }

    pub async fn next_queued_job(&self, batch: Option<i64>) -> Result<Option<JobRow>, StoreError> {
        let row = sqlx::query(&format!(
            "SELECT {JOB_PROJECTION} FROM jobs \
         WHERE status = 'queued' AND (?1 IS NULL OR batch_id = ?1) \
         ORDER BY id LIMIT 1"
        ))
        .bind(batch)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(row.map(JobRow::from_row))
    }

    pub async fn list_jobs(&self, batch: Option<i64>) -> Result<Vec<JobRow>, StoreError> {
        let rows = sqlx::query(&format!(
            "SELECT {JOB_PROJECTION} FROM jobs WHERE (?1 IS NULL OR batch_id = ?1) ORDER BY id"
        ))
        .bind(batch)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(rows.into_iter().map(JobRow::from_row).collect())
    }

    pub async fn claim_job(&self, id: i64) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE jobs SET status = 'running', attempts = attempts + 1, \
             updated_at = datetime('now') WHERE id = ?1",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(())
    }

    /// The stage doubles as the resume checkpoint.
    pub async fn set_job_stage(&self, id: i64, stage: &str) -> Result<(), StoreError> {
        sqlx::query("UPDATE jobs SET stage = ?2, updated_at = datetime('now') WHERE id = ?1")
            .bind(id)
            .bind(stage)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(())
    }

    pub async fn mark_job_done(&self, id: i64) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE jobs SET status = 'done', stage = 'done', \
             updated_at = datetime('now') WHERE id = ?1",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(())
    }

    pub async fn mark_job_failed(
        &self,
        id: i64,
        error_class: &str,
        error_msg: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE jobs SET status = 'failed', error_class = ?2, error_msg = ?3, \
             updated_at = datetime('now') WHERE id = ?1",
        )
        .bind(id)
        .bind(error_class)
        .bind(truncate(error_msg, 2000))
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(())
    }

    /// Crash recovery: requeue jobs stuck in `running`, keeping their stage checkpoint.
    pub async fn reset_stale_running(&self) -> Result<u64, StoreError> {
        let result = sqlx::query(
            "UPDATE jobs SET status = 'queued', updated_at = datetime('now') \
             WHERE status = 'running'",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(result.rows_affected())
    }

    pub async fn list_artifacts(&self, job_id: i64) -> Result<Vec<ArtifactRow>, StoreError> {
        let rows = sqlx::query(
            "SELECT kind, path, lang FROM artifacts WHERE job_id = ?1 \
             ORDER BY kind, lang",
        )
            .bind(job_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(rows
            .into_iter()
            .map(|r| ArtifactRow {
                kind: r.get("kind"),
                path: r.get("path"),
                lang: r.get("lang"),
            })
            .collect())
    }

    /// Drops the records for the given translations and returns their paths for
    /// the caller to delete. The source row (`lang = ''`) can never match.
    pub async fn remove_translation(
        &self,
        job_id: i64,
        targets: &[String],
    ) -> Result<Vec<std::path::PathBuf>, StoreError> {
        let mut removed = Vec::new();
        for lang in targets {
            let rows = sqlx::query(
                "SELECT path FROM artifacts WHERE job_id = ?1 AND lang = ?2 \
                 AND lang <> ''",
            )
            .bind(job_id)
            .bind(lang)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
            for row in rows {
                removed.push(std::path::PathBuf::from(row.get::<String, _>("path")));
            }
            sqlx::query("DELETE FROM artifacts WHERE job_id = ?1 AND lang = ?2 AND lang <> ''")
                .bind(job_id)
                .bind(lang)
                .execute(&self.pool)
                .await
                .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        }
        Ok(removed)
    }

    pub async fn job_stage(&self, id: i64) -> Result<Option<String>, StoreError> {
        let row = sqlx::query("SELECT stage FROM jobs WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(row.map(|r| r.get("stage")))
    }

    pub async fn job_by_id(&self, id: i64) -> Result<Option<JobRow>, StoreError> {
        let row = sqlx::query(&format!(
            "SELECT {JOB_PROJECTION} FROM jobs WHERE id = ?1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(row.map(JobRow::from_row))
    }

    /// Keeps `attempts` and the stage checkpoint so the retry resumes where it failed.
    pub async fn requeue_job(&self, id: i64) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE jobs SET status = 'queued', error_class = NULL, error_msg = NULL, \
             updated_at = datetime('now') \
             WHERE id = ?1 AND status = 'failed'",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn requeue_failed(&self, batch: Option<i64>) -> Result<u64, StoreError> {
        let result = sqlx::query(
            "UPDATE jobs SET status = 'queued', error_class = NULL, error_msg = NULL, \
             updated_at = datetime('now') \
             WHERE status = 'failed' AND (?1 IS NULL OR batch_id = ?1)",
        )
        .bind(batch)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(result.rows_affected())
    }

    /// Deletes records only, not files. Used before a rerun so stale rows don't
    /// shadow the new output.
    pub async fn clear_artifacts(&self, job_id: i64) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM artifacts WHERE job_id = ?1")
            .bind(job_id)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(())
    }

    /// Refuses running jobs; those belong to the queue worker.
    pub async fn remove_job(&self, id: i64) -> Result<bool, StoreError> {
        self.remove_job_inner(id, false).await
    }

    /// Deletes the row even while running: the worker finds nothing to update and
    /// moves on to the next job, so one job is cancelled without stopping the queue.
    pub async fn cancel_job(&self, id: i64) -> Result<bool, StoreError> {
        self.remove_job_inner(id, true).await
    }

    async fn remove_job_inner(&self, id: i64, include_running: bool) -> Result<bool, StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        sqlx::query("DELETE FROM artifacts WHERE job_id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        let sql = if include_running {
            "DELETE FROM jobs WHERE id = ?1"
        } else {
            "DELETE FROM jobs WHERE id = ?1 AND status != 'running'"
        };
        let result = sqlx::query(sql)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        tx.commit()
            .await
            .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(result.rows_affected() > 0)
    }

    /// `lang` is empty for the source artifact.
    pub async fn add_artifact(
        &self,
        job_id: i64,
        kind: &str,
        lang: &str,
        path: &Path,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO artifacts (job_id, kind, lang, path) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(job_id, kind, lang) DO UPDATE SET path = excluded.path",
        )
        .bind(job_id)
        .bind(kind)
        .bind(lang)
        .bind(path.to_string_lossy().as_ref())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(())
    }

    pub async fn usage_totals(&self) -> Result<Vec<UsageRow>, StoreError> {
        let rows = sqlx::query(
            "SELECT stage, provider, model, COUNT(*) AS calls, \
             SUM(tokens_in) AS tokens_in, SUM(tokens_out) AS tokens_out \
             FROM usage GROUP BY stage, provider, model ORDER BY stage",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Sqlite(self.path.clone(), e))?;
        Ok(rows
            .into_iter()
            .map(|r| UsageRow {
                stage: r.get("stage"),
                provider: r.get("provider"),
                model: r.get("model"),
                calls: r.get::<i64, _>("calls") as u64,
                tokens_in: r.get::<i64, _>("tokens_in") as u64,
                tokens_out: r.get::<i64, _>("tokens_out") as u64,
            })
            .collect())
    }
}

const JOB_PROJECTION: &str = "id, batch_id, source_kind, source_ref, stage, status, \
     error_class, error_msg, attempts, out_dir";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct JobRow {
    pub id: i64,
    pub batch_id: i64,
    pub source_kind: String,
    pub source_ref: String,
    pub stage: String,
    /// queued | running | done | failed | cancelled
    pub status: String,
    /// `transient` | `permanent` when failed, else NULL.
    pub error_class: Option<String>,
    pub error_msg: Option<String>,
    pub attempts: u32,
    pub out_dir: String,
}

impl JobRow {
    fn from_row(r: sqlx::sqlite::SqliteRow) -> Self {
        use sqlx::Row;
        JobRow {
            id: r.get::<i64, _>("id"),
            batch_id: r.get::<i64, _>("batch_id"),
            source_kind: r.get("source_kind"),
            source_ref: r.get("source_ref"),
            stage: r.get("stage"),
            status: r.get("status"),
            error_class: r.get("error_class"),
            error_msg: r.get("error_msg"),
            attempts: r.get::<i64, _>("attempts") as u32,
            out_dir: r.get("out_dir"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ArtifactRow {
    pub kind: String,
    pub path: String,
    /// Empty for the source artifact.
    pub lang: String,
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut cut = max;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\u{2026}", &text[..cut])
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub stage: String,
    pub provider: String,
    pub model: String,
    pub calls: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "subtitler-store-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("store.db")
    }

    #[tokio::test]
    async fn llm_cache_roundtrip_and_key_separation() {
        let path = temp_store("llm");
        let store = Store::open(&path).await.unwrap();
        assert!(store.llm_get("k1").await.unwrap().is_none());
        store.llm_put("k1", "{}", 10, 5).await.unwrap();
        let entry = store.llm_get("k1").await.unwrap().unwrap();
        assert_eq!(
            (entry.tokens_in, entry.tokens_out, entry.response.as_str()),
            (10, 5, "{}")
        );

        // Labelled parts: "ab"+"c" must not collide with "a"+"bc".
        assert_ne!(
            Store::cache_key(&["ab", "c"]),
            Store::cache_key(&["a", "bc"])
        );
        assert_eq!(Store::cache_key(&["a", "b"]), Store::cache_key(&["a", "b"]));
    }

    #[tokio::test]
    async fn stt_cache_roundtrip() {
        let path = temp_store("stt");
        let store = Store::open(&path).await.unwrap();
        store
            .stt_put("audio-hash", "{\"words\": []}")
            .await
            .unwrap();
        assert_eq!(
            store.stt_get("audio-hash").await.unwrap().as_deref(),
            Some("{\"words\": []}")
        );
        assert!(store.stt_get("other").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn usage_rows_aggregate() {
        let path = temp_store("usage");
        let store = Store::open(&path).await.unwrap();
        store
            .usage(
                "translate",
                "openrouter",
                "deepseek/deepseek-v4.1-flash",
                100,
                200,
            )
            .await
            .unwrap();
        store
            .usage(
                "translate",
                "openrouter",
                "deepseek/deepseek-v4.1-flash",
                50,
                60,
            )
            .await
            .unwrap();
        let rows = store.usage_totals().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].calls, rows[0].tokens_in, rows[0].tokens_out),
            (2, 150, 260)
        );
    }

    #[tokio::test]
    async fn reopening_persists_data() {
        let path = temp_store("reopen");
        {
            let store = Store::open(&path).await.unwrap();
            store.llm_put("persist", "ok", 1, 2).await.unwrap();
        }
        let store = Store::open(&path).await.unwrap();
        assert!(store.llm_get("persist").await.unwrap().is_some());
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;

    fn temp_store(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "subtitler-queue-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("store.db")
    }

    #[tokio::test]
    async fn enqueue_claim_complete_lifecycle() {
        let store = Store::open(&temp_store("life")).await.unwrap();
        assert!(
            store.next_queued_job(None).await.unwrap().is_none(),
            "empty queue"
        );

        let batch = store.create_batch("{}").await.unwrap();
        let j1 = store
            .enqueue_job(
                batch,
                "file",
                "/tmp/a.mp4",
                std::path::Path::new("data/b/1"),
            )
            .await
            .unwrap();
        let j2 = store
            .enqueue_job(
                batch,
                "url",
                "https://example.com/v",
                std::path::Path::new("data/b/2"),
            )
            .await
            .unwrap();
        assert_ne!(j1, j2);

        let first = store.next_queued_job(None).await.unwrap().unwrap();
        assert_eq!((first.id, first.source_kind.as_str()), (j1, "file"));
        store.claim_job(first.id).await.unwrap();
        store.set_job_stage(first.id, "transcribing").await.unwrap();
        assert_eq!(
            store.job_stage(first.id).await.unwrap().as_deref(),
            Some("transcribing")
        );
        store.mark_job_done(first.id).await.unwrap();

        // Failed jobs are not re-served.
        let second = store.next_queued_job(None).await.unwrap().unwrap();
        assert_eq!(second.id, j2);
        store.claim_job(second.id).await.unwrap();
        store
            .mark_job_failed(second.id, "permanent", "private video")
            .await
            .unwrap();
        assert!(store.next_queued_job(None).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn stale_running_jobs_are_requeued() {
        let store = Store::open(&temp_store("stale")).await.unwrap();
        let batch = store.create_batch("{}").await.unwrap();
        let id = store
            .enqueue_job(batch, "file", "x", std::path::Path::new("o"))
            .await
            .unwrap();
        store.claim_job(id).await.unwrap();
        store.set_job_stage(id, "transcribing").await.unwrap();
        assert_eq!(store.reset_stale_running().await.unwrap(), 1);
        let again = store.next_queued_job(None).await.unwrap().unwrap();
        assert_eq!(again.id, id);
        assert_eq!(again.stage, "transcribing");
    }

    #[tokio::test]
    async fn artifacts_upsert_per_kind_and_lang() {
        let store = Store::open(&temp_store("art")).await.unwrap();
        let batch = store.create_batch("{}").await.unwrap();
        let id = store
            .enqueue_job(batch, "file", "x", std::path::Path::new("o"))
            .await
            .unwrap();
        store
            .add_artifact(id, "srt", "tr", std::path::Path::new("o/tr.srt"))
            .await
            .unwrap();
        store
            .add_artifact(id, "srt", "tr", std::path::Path::new("o/tr-v2.srt"))
            .await
            .unwrap();

        let mut rows = store.list_artifacts(id).await.unwrap();
        rows.retain(|r| r.kind == "srt");
        assert_eq!(rows.len(), 1, "aynı dil yeniden yazılınca tek satır kalır");
        assert_eq!(rows[0].path, "o/tr-v2.srt");
        assert_eq!(rows[0].lang, "tr");
    }

    // Regression: the old `(job_id, kind)` key let a translation overwrite the source row.
    #[tokio::test]
    async fn a_translation_never_overwrites_the_source_track() {
        let store = Store::open(&temp_store("track-keep")).await.unwrap();
        let batch = store.create_batch("{}").await.unwrap();
        let id = store
            .enqueue_job(batch, "file", "x", std::path::Path::new("o"))
            .await
            .unwrap();
        store
            .add_artifact(id, "track", "", std::path::Path::new("o/source.track.json"))
            .await
            .unwrap();
        for lang in ["en", "tr", "az"] {
            store
                .add_artifact(id, "track", lang, std::path::Path::new(&format!("o/{lang}.track.json")))
                .await
                .unwrap();
        }

        let tracks = store.list_artifacts(id).await.unwrap();
        let tracks: Vec<_> = tracks.into_iter().filter(|r| r.kind == "track").collect();
        assert_eq!(tracks.len(), 4, "kaynak + 3 çeviri birlikte durmalı");
        assert_eq!(
            tracks.iter().filter(|t| t.lang.is_empty()).count(),
            1,
            "tam bir kaynak transcript kalmalı"
        );
        assert!(
            tracks.iter().any(|t| t.path.ends_with("source.track.json")),
            "kaynak transcript yolu kaybolmamalı"
        );
        for lang in ["en", "tr", "az"] {
            assert!(
                tracks.iter().any(|t| t.lang == lang),
                "{lang} çevirisi kayıtlı olmalı"
            );
        }
    }

    #[tokio::test]
    async fn requeue_failed_preserves_attempts_and_respects_batch_scope() {
        let store = Store::open(&temp_store("requeue")).await.unwrap();
        let batch = store.create_batch("{}").await.unwrap();
        let other = store.create_batch("{}").await.unwrap();
        let mut ids = Vec::new();
        for (b, n) in [(batch, 1), (batch, 2), (other, 3)] {
            ids.push(
                store
                    .enqueue_job(
                        b,
                        "url",
                        &format!("https://x/{n}"),
                        std::path::Path::new("o"),
                    )
                    .await
                    .unwrap(),
            );
        }
        let [j1, j2, j3] = [ids[0], ids[1], ids[2]];
        for id in [j1, j2, j3] {
            store.claim_job(id).await.unwrap(); // attempts → 1
            store
                .mark_job_failed(id, "transient", "http error 429")
                .await
                .unwrap();
        }
        assert!(
            store.next_queued_job(None).await.unwrap().is_none(),
            "failed jobs are not served before requeue"
        );

        assert_eq!(store.requeue_failed(Some(batch)).await.unwrap(), 2);
        let first = store.next_queued_job(Some(batch)).await.unwrap().unwrap();
        assert_eq!(first.id, j1);
        assert_eq!(first.attempts, 1, "attempts preserved across requeue");
        assert_eq!(first.status, "queued");
        assert_eq!(first.error_class, None, "error fields cleared on requeue");
        assert_eq!(first.error_msg, None);
        store.claim_job(first.id).await.unwrap();
        let second = store.next_queued_job(Some(batch)).await.unwrap().unwrap();
        assert_eq!(second.id, j2);
        store.claim_job(second.id).await.unwrap();
        assert!(store.next_queued_job(Some(batch)).await.unwrap().is_none());

        let other_jobs = store.list_jobs(Some(other)).await.unwrap();
        assert_eq!(other_jobs.len(), 1);
        assert_eq!(other_jobs[0].status, "failed");
        assert_eq!(other_jobs[0].error_class.as_deref(), Some("transient"));

        assert_eq!(store.requeue_failed(None).await.unwrap(), 1);
        assert!(store.next_queued_job(None).await.unwrap().is_some());
        assert_eq!(store.requeue_failed(None).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn list_jobs_orders_and_projects_status_fields() {
        let store = Store::open(&temp_store("list")).await.unwrap();
        let batch = store.create_batch("{}").await.unwrap();
        let other = store.create_batch("{}").await.unwrap();
        let done_job = store
            .enqueue_job(batch, "file", "a", std::path::Path::new("o/a"))
            .await
            .unwrap();
        let failed_job = store
            .enqueue_job(batch, "url", "https://x/b", std::path::Path::new("o/b"))
            .await
            .unwrap();
        let queued_job = store
            .enqueue_job(other, "file", "c", std::path::Path::new("o/c"))
            .await
            .unwrap();
        store.claim_job(done_job).await.unwrap();
        store.mark_job_done(done_job).await.unwrap();
        store.claim_job(failed_job).await.unwrap();
        store
            .mark_job_failed(failed_job, "permanent", "private video")
            .await
            .unwrap();

        let jobs = store.list_jobs(None).await.unwrap();
        assert_eq!(jobs.len(), 3);
        assert!(jobs.windows(2).all(|w| w[0].id < w[1].id), "ORDER BY id");
        let done = jobs.iter().find(|j| j.id == done_job).unwrap();
        assert_eq!(
            (done.status.as_str(), done.stage.as_str()),
            ("done", "done")
        );
        let failed = jobs.iter().find(|j| j.id == failed_job).unwrap();
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error_class.as_deref(), Some("permanent"));
        assert_eq!(failed.error_msg.as_deref(), Some("private video"));
        assert_eq!(failed.attempts, 1);
        let queued = jobs.iter().find(|j| j.id == queued_job).unwrap();
        assert_eq!(queued.status, "queued");
        assert_eq!(
            (queued.error_class.as_deref(), queued.error_msg.as_deref()),
            (None, None)
        );

        assert_eq!(store.list_jobs(Some(batch)).await.unwrap().len(), 2);
        assert_eq!(store.list_jobs(Some(other)).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn opening_repairs_stage_of_finished_jobs() {
        let path = temp_store("stage-heal");
        let store = Store::open(&path).await.unwrap();
        let batch = store.create_batch("{}").await.unwrap();
        let job = store
            .enqueue_job(batch, "url", "https://example.com/v.mp4", std::path::Path::new("out"))
            .await
            .unwrap();

        store.set_job_stage(job, "exporting").await.unwrap();
        store.mark_job_done(job).await.unwrap();
        // Recreate the legacy state by hand; `mark_job_done` already fixes the stage.
        sqlx::query("UPDATE jobs SET stage = 'exporting' WHERE id = ?1")
            .bind(job)
            .execute(&store.pool)
            .await
            .unwrap();

        let reopened = Store::open(&path).await.unwrap();
        let stage: String =
            sqlx::query_scalar("SELECT stage FROM jobs WHERE id = ?1")
                .bind(job)
                .fetch_one(&reopened.pool)
                .await
                .unwrap();
        assert_eq!(stage, "done", "terminal işte aşama da terminal olmalı");
    }

    #[tokio::test]
    async fn migration_repairs_a_legacy_store() {
        let dir = std::env::temp_dir().join(format!(
            "subtitler-legacy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("store.db");
        let out = dir.join("job");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("source.track.json"), r#"{"lang":"en","cues":[]}"#).unwrap();

        // Hand-built legacy schema; `Store::open` should migrate it.
        {
            let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db.display()))
                .await
                .unwrap();
            sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
            sqlx::query(
                "CREATE TABLE batches (\
                     id INTEGER PRIMARY KEY AUTOINCREMENT,\
                     settings_json TEXT NOT NULL DEFAULT '{}',\
                     created_at TEXT NOT NULL DEFAULT (datetime('now')))",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE jobs (\
                     id INTEGER PRIMARY KEY AUTOINCREMENT,\
                     batch_id INTEGER NOT NULL REFERENCES batches(id),\
                     source_kind TEXT NOT NULL CHECK (source_kind IN ('file', 'url')),\
                     source_ref TEXT NOT NULL,\
                     stage TEXT NOT NULL DEFAULT 'queued',\
                     status TEXT NOT NULL DEFAULT 'queued'\
                         CHECK (status IN ('queued', 'running', 'done', 'failed', 'cancelled')),\
                     error_class TEXT,\
                     error_msg TEXT,\
                     attempts INTEGER NOT NULL DEFAULT 0,\
                     out_dir TEXT NOT NULL,\
                     created_at TEXT NOT NULL DEFAULT (datetime('now')),\
                     updated_at TEXT NOT NULL DEFAULT (datetime('now')))",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE artifacts (\
                     job_id INTEGER NOT NULL REFERENCES jobs(id),\
                     kind TEXT NOT NULL,\
                     path TEXT NOT NULL,\
                     created_at TEXT NOT NULL DEFAULT (datetime('now')),\
                     PRIMARY KEY (job_id, kind))",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO batches (settings_json) VALUES ('{}')")
                .execute(&pool)
                .await
                .unwrap();
            let batch: i64 = sqlx::query_scalar(
                "INSERT INTO batches (settings_json) VALUES ('{}') RETURNING id",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            let job: i64 = sqlx::query_scalar(
                "INSERT INTO jobs (batch_id, source_kind, source_ref, out_dir, status) \
                 VALUES (?1, 'file', 'v.mp4', ?2, 'done') RETURNING id",
            )
            .bind(batch)
            .bind(out.to_string_lossy().as_ref())
            .fetch_one(&pool)
            .await
            .unwrap();
            // The old bug: a translation sitting in the source's slot.
            sqlx::query("INSERT INTO artifacts (job_id, kind, path) VALUES (?1, 'track', ?2)")
                .bind(job)
                .bind(out.join("az.track.json").to_string_lossy().as_ref())
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        let store = Store::open(&db).await.unwrap();
        let job_id: i64 = sqlx::query_scalar("SELECT MIN(id) FROM jobs")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        let tracks: Vec<_> = store
            .list_artifacts(job_id)
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.kind == "track")
            .collect();
        assert!(
            tracks.iter().any(|t| t.lang == "az"),
            "çeviri kaydı korunmalı: {tracks:?}"
        );
        assert!(
            tracks
                .iter()
                .any(|t| t.lang.is_empty() && t.path.ends_with("source.track.json")),
            "kaynak transcript kaydı geri gelmeli: {tracks:?}"
        );
    }

    // Regression: translations were written to disk but never registered, and the
    // UI only lists what's registered.
    #[tokio::test]
    async fn translations_are_registered_so_the_ui_can_list_them() {
        let store = Store::open(&temp_store("trans-register")).await.unwrap();
        let batch = store.create_batch("{}").await.unwrap();
        let id = store
            .enqueue_job(batch, "file", "x", std::path::Path::new("o"))
            .await
            .unwrap();

        store
            .add_artifact(id, "track", "", std::path::Path::new("o/source.track.json"))
            .await
            .unwrap();
        for lang in ["tr", "de"] {
            store
                .add_artifact(id, "track", lang, std::path::Path::new(&format!("o/{lang}.track.json")))
                .await
                .unwrap();
            store
                .add_artifact(id, "srt", lang, std::path::Path::new(&format!("o/{lang}.srt")))
                .await
                .unwrap();
        }

        let all = store.list_artifacts(id).await.unwrap();
        let tracks: Vec<_> = all.iter().filter(|a| a.kind == "track").collect();
        assert_eq!(tracks.len(), 3, "kaynak + 2 çeviri görünmeli: {tracks:?}");
        assert_eq!(
            tracks.iter().filter(|t| t.lang.is_empty()).count(),
            1,
            "tam bir kaynak kaydı olmalı"
        );
        for lang in ["tr", "de"] {
            assert!(tracks.iter().any(|t| t.lang == lang), "{lang} görünmeli");
            assert!(all.iter().any(|a| a.kind == "srt" && a.lang == lang));
        }
    }
}
