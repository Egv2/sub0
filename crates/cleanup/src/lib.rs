//! LLM disfluency cleanup. The model only returns word indices to delete, so
//! text is never rewritten and surviving words keep their timestamps.

pub mod rules;

use subtitler_core::model::Word;
use subtitler_translate::LlmProvider;

const WINDOW: usize = 250;
const CONTEXT: usize = 15;

#[derive(Clone)]
pub struct CleanupOptions<'a> {
    pub provider: &'a dyn LlmProvider,
    pub model: &'a str,
    pub prompt: &'a str,
    pub lang: &'a str,
    /// Above this share of deletions per window, only non-lexical deletions are
    /// kept.
    pub max_deletion_ratio: f32,
    pub cache: Option<&'a subtitler_store::Store>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanupStats {
    pub llm_calls: u32,
    pub cache_hits: u32,
}

#[derive(Debug, Default)]
pub struct CleanupResult {
    pub kept: Vec<Word>,
    /// Sorted; persisted so a deletion can be audited.
    pub deleted_indices: Vec<usize>,
    pub flagged_windows: Vec<usize>,
    pub stats: CleanupStats,
}

#[derive(Debug, thiserror::Error)]
pub enum CleanupError {
    #[error("LLM call failed: {0}")]
    Llm(#[from] subtitler_translate::LlmError),
    #[error("cleanup produced invalid JSON after retries: {0}")]
    BadResponse(String),
}

/// Never fails on bad model output: broken windows are kept intact and listed
/// in `flagged_windows`.
pub async fn cleanup_words(
    words: &[Word],
    opts: &CleanupOptions<'_>,
) -> Result<CleanupResult, CleanupError> {
    let mut result = CleanupResult::default();
    if words.is_empty() {
        return Ok(result);
    }

    let mut deleted = vec![false; words.len()];
    let mut stats = CleanupStats::default();
    let mut start = 0usize;
    while start < words.len() {
        let end = (start + WINDOW).min(words.len());
        match cleanup_window(words, start, end, opts, &mut stats).await {
            Ok((indices, over_cap)) => {
                for i in indices {
                    deleted[i] = true;
                }
                if over_cap {
                    result.flagged_windows.push(start);
                }
            }
            Err(window_err) => {
                eprintln!(
                    "[cleanup] window {start}..{end} rejected ({window_err}); keeping words intact"
                );
                result.flagged_windows.push(start);
            }
        }
        start = end;
    }

    result.deleted_indices = (0..words.len()).filter(|&i| deleted[i]).collect();
    result.kept = words
        .iter()
        .enumerate()
        .filter(|(i, _)| !deleted[*i])
        .map(|(_, w)| w.clone())
        .collect();
    result.stats = stats;
    Ok(result)
}

async fn cleanup_window(
    words: &[Word],
    start: usize,
    end: usize,
    opts: &CleanupOptions<'_>,
    stats: &mut CleanupStats,
) -> Result<(Vec<usize>, bool), String> {
    let word_json: Vec<serde_json::Value> = words[start..end]
        .iter()
        .enumerate()
        .map(|(i, w)| serde_json::json!({ "i": start + i, "w": w.text }))
        .collect();
    let before: Vec<serde_json::Value> = words[start.saturating_sub(CONTEXT)..start]
        .iter()
        .enumerate()
        .map(|(i, w)| serde_json::json!({ "i": start.saturating_sub(CONTEXT) + i, "w": w.text }))
        .collect();
    let after: Vec<serde_json::Value> = words[end..(end + CONTEXT).min(words.len())]
        .iter()
        .enumerate()
        .map(|(i, w)| serde_json::json!({ "i": end + i, "w": w.text }))
        .collect();
    let user = serde_json::json!({
        "context_before": before,
        "words": word_json,
        "context_after": after,
    })
    .to_string();

    let mut content: Option<String> = None;
    let cache_key = opts.cache.as_ref().map(|_store| {
        let prompt_hash = subtitler_store::Store::hash_hex(opts.prompt.as_bytes());
        subtitler_store::Store::cache_key(&["cleanup", &prompt_hash, opts.model, &user])
    });
    if let (Some(store), Some(key)) = (opts.cache, cache_key.as_deref()) {
        if let Ok(Some(entry)) = store.llm_get(key).await {
            stats.cache_hits += 1;
            content = Some(entry.response);
        }
    }
    if content.is_none() {
        let mut fetched = None;
        for _ in 0..3 {
            match subtitler_translate::chat_json_with_retries(
                opts.provider,
                opts.model,
                opts.prompt,
                &user,
            )
            .await
            {
                Ok((c, _usage)) => {
                    stats.llm_calls += 1;
                    fetched = Some(c);
                    break;
                }
                Err(e) if e.is_transient() => { /* retried inside with backoff */ }
                Err(e) => return Err(format!("LLM: {e}")),
            }
        }
        content = fetched;
        if let (Some(store), Some(key), Some(text)) =
            (opts.cache, cache_key.as_deref(), content.as_ref())
        {
            if let Err(e) = store.llm_put(key, text, 0, 0).await {
                eprintln!("[cleanup] cache write failed: {e}");
            }
            let _ = store
                .usage("cleanup", opts.provider.name(), opts.model, 0, 0)
                .await;
        }
    }
    let content = content.ok_or_else(|| "no usable response".to_string())?;

    let value: serde_json::Value =
        serde_json::from_str(strip_fences(&content)).map_err(|e| format!("invalid JSON: {e}"))?;
    let list = value
        .get("delete")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "missing delete[]".to_string())?;

    let mut indices = Vec::new();
    for v in list {
        let Some(i) = v.as_u64().map(|n| n as usize) else {
            return Err("non-integer index".into());
        };
        if i < start || i >= end {
            // An index outside the window means the model lost track of the format.
            return Err(format!("out-of-range index {i}"));
        }
        indices.push(i);
    }
    indices.sort_unstable();
    indices.dedup();

    let ratio = indices.len() as f32 / (end - start) as f32;
    let over_cap = ratio > opts.max_deletion_ratio;
    if over_cap {
        eprintln!(
            "[cleanup] window {start}..{end}: deletion ratio {ratio:.2} exceeds cap {:.2}; keeping only non-lexical deletions",
            opts.max_deletion_ratio
        );
        indices.retain(|&i| rules::is_nonlexical(&words[i].text, opts.lang));
    }
    Ok((indices, over_cap))
}

fn strip_fences(content: &str) -> &str {
    let mut s = content.trim();
    if let Some(rest) = s.strip_prefix("```") {
        s = rest.trim_start_matches("json").trim_start();
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.trim_end();
    }
    s
}

#[cfg(test)]
mod tests;
