//! Batched id→text translation with a validator and a retry ladder.

use std::collections::HashMap;

use subtitler_core::model::{Cue, CueFlag, Track, TrackFlag};

use crate::brief::Brief;
use crate::provider::LlmProvider;

/// Keep in sync with the version line in prompts/translate.system.md.
pub const PROMPT_VERSION: &str = "3";

const BASE_SYSTEM_PROMPT: &str = include_str!("../../../prompts/translate.system.md");

/// Reasoning models spend hidden tokens per batch; bigger batches started
/// truncating responses.
const MAX_BATCH: usize = 20;
const CONTEXT_CUES: usize = 3;
const PRO_CONTEXT_BEFORE: usize = 6;
const SINGLE_CONTEXT_CUES: usize = 3;
const MAX_GROUP_CUES: usize = 4;
const BATCH_ATTEMPTS: u32 = 3;
const SINGLE_ATTEMPTS: u32 = 2;
/// Deliberately loose; only catches runaway or truncated output.
const LEN_RATIO_MIN: f32 = 0.2;
const LEN_RATIO_MAX: f32 = 5.0;
/// Cues shorter than this may legitimately come back unchanged ("OK", names).
const ECHO_MIN_WORDS: usize = 2;
const ECHO_MIN_CHARS: usize = 10;

pub fn system_prompt(user_prompt: &str) -> String {
    system_prompt_with(user_prompt, None)
}

/// Every batch of a track gets the exact same system prompt, which keeps
/// provider-side prefix caching effective.
pub fn system_prompt_with(user_prompt: &str, brief: Option<&Brief>) -> String {
    let user = if user_prompt.trim().is_empty() {
        "(no special style directives)"
    } else {
        user_prompt.trim()
    };
    let localization = brief.map(Brief::localization_block).unwrap_or_default();
    BASE_SYSTEM_PROMPT
        .replace("{{LOCALIZATION}}", &localization)
        .replace("{{USER_TRANSLATION_PROMPT}}", user)
}

pub struct TranslateOptions<'a> {
    pub provider: &'a dyn LlmProvider,
    pub model: &'a str,
    pub target_lang: &'a str,
    pub user_prompt: &'a str,
    pub cache: Option<&'a subtitler_store::Store>,
    pub brief: Option<&'a Brief>,
    /// Send earlier cues with their chosen translations (pro mode), so names
    /// and forms of address stay consistent.
    pub prior_context: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TranslateStats {
    pub llm_calls: u32,
    pub cache_hits: u32,
}

/// Never fails: cues that can't be recovered keep their source text and a
/// `TranslationFailed` flag. Ids and times are copied from the source track,
/// so the model can't shift them.
pub async fn translate_track(
    source: &Track,
    opts: TranslateOptions<'_>,
) -> (Track, TranslateStats) {
    let system = system_prompt_with(opts.user_prompt, opts.brief);
    let groups = sentence_groups(&source.cues);
    let mut stats = TranslateStats::default();
    let mut out = Track {
        lang: opts.target_lang.to_string(),
        cues: Vec::with_capacity(source.cues.len()),
        flags: Vec::new(),
    };

    let mut start = 0usize;
    while start < source.cues.len() {
        let end = batch_end(&groups, start);
        let batch = &source.cues[start..end];
        let batch_groups = &groups[start..end];
        let ids: Vec<u32> = batch.iter().map(|c| c.id).collect();

        let prior = opts.prior_context.then_some(&out);
        let user = batch_user_message(opts.target_lang, source, &groups, start, end, prior);
        let cache_key = opts.cache.as_ref().map(|_store| {
            let system_hash = blake3::hash(system.as_bytes()).to_string();
            subtitler_store::Store::cache_key(&[
                "translate",
                PROMPT_VERSION,
                &system_hash,
                opts.model,
                opts.target_lang,
                &user,
            ])
        });

        let mut translated: Option<HashMap<u32, String>> = None;
        if let (Some(store), Some(key)) = (opts.cache, cache_key.as_deref()) {
            match store.llm_get(key).await {
                Ok(Some(entry)) => match validate_grouped(&ids, batch, batch_groups, &entry.response) {
                    Ok(map) => {
                        stats.cache_hits += 1;
                        translated = Some(map);
                    }
                    Err(_) => { /* stale/broken entry: fall through to the LLM */ }
                },
                Ok(None) => {}
                Err(e) => eprintln!("[translate] cache read failed: {e}"),
            }
        }

        if translated.is_none() {
            for attempt in 0..BATCH_ATTEMPTS {
                match chat_with_retries(&opts, &system, &user).await {
                    Ok((content, usage)) => {
                        stats.llm_calls += 1;
                        if let Some(store) = opts.cache {
                            let tokens_in = usage.as_ref().map(|u| u.input_tokens).unwrap_or(0);
                            let tokens_out = usage.as_ref().map(|u| u.output_tokens).unwrap_or(0);
                            // Rejected responses still cost tokens, so record usage either way.
                            let _ = store
                                .usage(
                                    "translate",
                                    opts.provider.name(),
                                    opts.model,
                                    tokens_in,
                                    tokens_out,
                                )
                                .await;
                        }
                        match validate_grouped(&ids, batch, batch_groups, &content) {
                            Ok(map) => {
                                // Only cache validated output, or a rejected echo gets replayed forever.
                                if let (Some(store), Some(key)) =
                                    (opts.cache, cache_key.as_deref())
                                {
                                    let tokens_in = usage.as_ref().map(|u| u.input_tokens).unwrap_or(0);
                                    let tokens_out =
                                        usage.as_ref().map(|u| u.output_tokens).unwrap_or(0);
                                    if let Err(e) =
                                        store.llm_put(key, &content, tokens_in, tokens_out).await
                                    {
                                        eprintln!("[translate] cache write failed: {e}");
                                    }
                                }
                                translated = Some(map);
                                break;
                            }
                            Err(issues) => {
                                eprintln!(
                                    "[translate] batch at cue {} invalid (attempt {}/{}): {}",
                                    ids.first().unwrap_or(&0),
                                    attempt + 1,
                                    BATCH_ATTEMPTS,
                                    issues.join("; ")
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[translate] batch error ({}): {e}", opts.provider.name());
                        break;
                    }
                }
            }
        }

        match translated {
            Some(map) => {
                for cue in batch {
                    let text = map
                        .get(&cue.id)
                        .cloned()
                        .unwrap_or_else(|| cue.text.clone());
                    out.cues.push(retime(cue, text));
                }
            }
            None => {
                // Batch failed: fall back to one call per cue.
                for (offset, cue) in batch.iter().enumerate() {
                    match translate_single(&opts, &system, source, start + offset, &mut stats).await {
                        Ok(text) => {
                            out.cues.push(retime(cue, text));
                            out.flags.push(TrackFlag {
                                cue_id: cue.id,
                                flag: CueFlag::TranslationRetried,
                            });
                        }
                        Err(()) => {
                            out.cues.push(retime(cue, cue.text.clone()));
                            out.flags.push(TrackFlag {
                                cue_id: cue.id,
                                flag: CueFlag::TranslationFailed,
                            });
                        }
                    }
                }
            }
        }
        start = end;
    }
    (out, stats)
}

fn retime(source_cue: &Cue, text: String) -> Cue {
    Cue {
        id: source_cue.id,
        start_ms: source_cue.start_ms,
        end_ms: source_cue.end_ms,
        text,
    }
}

fn cue_json(cue: &Cue) -> serde_json::Value {
    serde_json::json!({ "id": cue.id, "text": cue.text })
}

fn ends_sentence(text: &str) -> bool {
    let t = text.trim_end().trim_end_matches(['"', '\'', '”', '’', '»', ')']);
    t.ends_with(['.', '!', '?', '…', '。', '！', '？', '؟'])
}

pub(crate) fn sentence_groups(cues: &[Cue]) -> Vec<u32> {
    let mut out = Vec::with_capacity(cues.len());
    let mut group = 0u32;
    let mut size = 0usize;
    for cue in cues {
        out.push(group);
        size += 1;
        if ends_sentence(&cue.text) || size >= MAX_GROUP_CUES {
            group += 1;
            size = 0;
        }
    }
    out
}

fn batch_end(groups: &[u32], start: usize) -> usize {
    let mut end = (start + MAX_BATCH).min(groups.len());
    while end < groups.len() && end > start && groups[end] == groups[end - 1] {
        end += 1;
    }
    end
}

fn batch_user_message(
    target_lang: &str,
    source: &Track,
    groups: &[u32],
    start: usize,
    end: usize,
    prior: Option<&Track>,
) -> String {
    let before_n = if prior.is_some() { PRO_CONTEXT_BEFORE } else { CONTEXT_CUES };
    let before: Vec<serde_json::Value> = (start.saturating_sub(before_n)..start)
        .map(|i| {
            let mut v = cue_json(&source.cues[i]);
            // Failed cues still hold source text; passing it on as a "translation" would mislead.
            let failed = |p: &Track, id: u32| {
                p.flags
                    .iter()
                    .any(|f| f.cue_id == id && f.flag == CueFlag::TranslationFailed)
            };
            if let Some(done) = prior.and_then(|p| p.cues.get(i).filter(|c| !failed(p, c.id))) {
                v["translation"] = done.text.clone().into();
            }
            v
        })
        .collect();
    let after: Vec<serde_json::Value> = source.cues
        [end..(end + CONTEXT_CUES).min(source.cues.len())]
        .iter()
        .map(cue_json)
        .collect();
    let translate: Vec<serde_json::Value> = (start..end)
        .map(|i| {
            let mut v = cue_json(&source.cues[i]);
            let g = groups[i];
            let shared = (i > 0 && groups[i - 1] == g) || groups.get(i + 1) == Some(&g);
            if shared {
                v["g"] = g.into();
            }
            v
        })
        .collect();
    serde_json::json!({
        "target_lang": target_lang,
        "context_before": before,
        "translate": translate,
        "context_after": after,
    })
    .to_string()
}

async fn chat_with_retries(
    opts: &TranslateOptions<'_>,
    system: &str,
    user: &str,
) -> Result<(String, Option<crate::provider::Usage>), crate::error::LlmError> {
    chat_json_with_retries(opts.provider, opts.model, system, user).await
}

/// Shared with cleanup.
pub async fn chat_json_with_retries(
    provider: &dyn LlmProvider,
    model: &str,
    system: &str,
    user: &str,
) -> Result<(String, Option<crate::provider::Usage>), crate::error::LlmError> {
    let mut attempt = 0u32;
    loop {
        match provider.chat_json(model, system, user, 0.0).await {
            Ok((content, usage)) => return Ok((content, usage)),
            Err(e) if e.is_transient() && attempt < 3 => {
                crate::provider::backoff_ms(attempt).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Single-cue fallback. Neighbours go along with the cue: a short line like
/// "How is that done?" is ambiguous on its own.
async fn translate_single(
    opts: &TranslateOptions<'_>,
    system: &str,
    source: &Track,
    index: usize,
    stats: &mut TranslateStats,
) -> Result<String, ()> {
    let cue = &source.cues[index];
    let before: Vec<serde_json::Value> = source.cues[index.saturating_sub(SINGLE_CONTEXT_CUES)..index]
        .iter()
        .map(cue_json)
        .collect();
    let after: Vec<serde_json::Value> = source.cues
        [(index + 1)..(index + 1 + SINGLE_CONTEXT_CUES).min(source.cues.len())]
        .iter()
        .map(cue_json)
        .collect();
    let user = serde_json::json!({
        "target_lang": opts.target_lang,
        "context_before": before,
        "translate": [cue_json(cue)],
        "context_after": after,
    })
    .to_string();
    let cache_key = opts.cache.as_ref().map(|_store| {
        let system_hash = blake3::hash(system.as_bytes()).to_string();
        subtitler_store::Store::cache_key(&[
            "translate",
            PROMPT_VERSION,
            &system_hash,
            opts.model,
            opts.target_lang,
            &user,
        ])
    });
    if let (Some(store), Some(key)) = (opts.cache, cache_key.as_deref()) {
        if let Ok(Some(entry)) = store.llm_get(key).await {
            if let Ok(map) = validate_batch(&[cue.id], std::slice::from_ref(cue), &entry.response) {
                if let Some(text) = map.get(&cue.id) {
                    stats.cache_hits += 1;
                    return Ok(text.clone());
                }
            }
        }
    }
    for _ in 0..SINGLE_ATTEMPTS {
        if let Ok((content, usage)) = chat_with_retries(opts, system, &user).await {
            stats.llm_calls += 1;
            let tokens_in = usage.as_ref().map(|u| u.input_tokens).unwrap_or(0);
            let tokens_out = usage.as_ref().map(|u| u.output_tokens).unwrap_or(0);
            if let Some(store) = opts.cache {
                let _ = store
                    .usage("translate", opts.provider.name(), opts.model, tokens_in, tokens_out)
                    .await;
            }
            if let Ok(map) = validate_batch(&[cue.id], std::slice::from_ref(cue), &content) {
                if let Some(text) = map.get(&cue.id) {
                    if let (Some(store), Some(key)) = (opts.cache, cache_key.as_deref()) {
                        if let Err(e) = store.llm_put(key, &content, tokens_in, tokens_out).await
                        {
                            eprintln!("[translate] cache write failed: {e}");
                        }
                    }
                    return Ok(text.clone());
                }
            }
        }
    }
    Err(())
}

/// Models add ```json fences despite being told not to.
fn unwrap_json(content: &str) -> &str {
    let mut s = content.trim();
    if let Some(rest) = s.strip_prefix("```") {
        let rest = rest.trim_start_matches("json").trim_start();
        s = rest;
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.trim_end();
    }
    s
}

/// A refusal or a confused model usually returns valid JSON with the source
/// text copied back, which passes every other check.
fn echoed(cue: &str, out: &str) -> bool {
    // Short cues may legitimately stay the same; failing on them would push a
    // whole batch into the single-cue path.
    let words = cue
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count();
    let chars = cue.chars().filter(|c| c.is_alphanumeric()).count();
    if words < ECHO_MIN_WORDS || chars < ECHO_MIN_CHARS {
        return false;
    }
    comparable(cue) == comparable(out)
}

fn comparable(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub(crate) fn validate_batch(
    requested: &[u32],
    batch: &[Cue],
    content: &str,
) -> Result<HashMap<u32, String>, Vec<String>> {
    let solo: Vec<u32> = (0..batch.len() as u32).collect();
    validate_grouped(requested, batch, &solo, content)
}

/// Length band is checked per sentence group, since text may move between
/// the cues of one group.
pub(crate) fn validate_grouped(
    requested: &[u32],
    batch: &[Cue],
    groups: &[u32],
    content: &str,
) -> Result<HashMap<u32, String>, Vec<String>> {
    let mut issues = Vec::new();
    let value: serde_json::Value = match serde_json::from_str(unwrap_json(content)) {
        Ok(v) => v,
        Err(e) => return Err(vec![format!("invalid JSON: {e}")]),
    };
    let items = match value.get("translations").and_then(|t| t.as_array()) {
        Some(a) => a,
        None => return Err(vec!["missing translations[]".into()]),
    };

    let mut map = HashMap::new();
    for item in items {
        let id = item.get("id").and_then(|v| v.as_u64());
        let text = item.get("text").and_then(|v| v.as_str());
        match (id, text) {
            (Some(id), Some(text)) => {
                let id = id as u32;
                if map.insert(id, text.to_string()).is_some() {
                    issues.push(format!("duplicate id {id}"));
                }
                if text.contains('\n') {
                    issues.push(format!("id {id}: line break in output"));
                }
                if text.trim().is_empty() {
                    issues.push(format!("id {id}: empty output"));
                }
            }
            _ => issues.push("item without id/text".into()),
        }
    }

    let mut group_len: HashMap<u32, (usize, usize, u32)> = HashMap::new();
    for (cue, &g) in batch.iter().zip(groups) {
        if let Some(text) = map.get(&cue.id) {
            let e = group_len.entry(g).or_insert((0, 0, cue.id));
            e.0 += cue.text.chars().count();
            e.1 += text.chars().count();
        }
    }
    for (src, out, first_id) in group_len.values() {
        if *src > 0 {
            let ratio = *out as f32 / *src as f32;
            if !(LEN_RATIO_MIN..=LEN_RATIO_MAX).contains(&ratio) {
                issues.push(format!("id {first_id}: length ratio {ratio:.2} outside band"));
            }
        }
    }
    for cue in batch {
        match map.get(&cue.id) {
            None => issues.push(format!("missing id {}", cue.id)),
            Some(text) => {
                if !cue.text.trim().is_empty() && echoed(&cue.text, text) {
                    issues.push(format!(
                        "id {}: output repeats the source text untranslated",
                        cue.id
                    ));
                }
            }
        }
    }
    for &id in map.keys() {
        if !requested.contains(&id) {
            issues.push(format!("extra id {id}"));
        }
    }

    if issues.is_empty() {
        Ok(map)
    } else {
        Err(issues)
    }
}

pub(crate) fn truncate_body(body: &str) -> String {
    const MAX: usize = 500;
    let body = body.trim();
    if body.len() <= MAX {
        body.to_string()
    } else {
        let mut cut = MAX;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}\u{2026}", &body[..cut])
    }
}
