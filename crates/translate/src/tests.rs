//! Mock errors are permanent: transient ones would make `chat_with_retries`
//! sleep for real.

use std::sync::Mutex;

use async_trait::async_trait;
use subtitler_core::model::{Cue, CueFlag, Track};
use subtitler_core::assert_timecodes_preserved;

use crate::provider::{LlmProvider, Usage};
use crate::translate::{sentence_groups, translate_track, validate_batch, validate_grouped, TranslateOptions};

#[derive(Default)]
struct MockLlm {
    /// Popped from the end; `Err` = permanent API error.
    script: Mutex<Vec<Result<String, ()>>>,
    seen: Mutex<Vec<String>>,
}

impl MockLlm {
    fn with(script: Vec<Result<String, ()>>) -> Self {
        Self { script: Mutex::new(script), seen: Mutex::new(Vec::new()) }
    }
}

#[async_trait]
impl LlmProvider for MockLlm {
    fn name(&self) -> &'static str {
        "mock"
    }
    async fn chat_json(
        &self,
        _model: &str,
        _system: &str,
        user: &str,
        _temperature: f32,
    ) -> Result<(String, Option<Usage>), crate::error::LlmError> {
        self.seen.lock().unwrap().push(user.to_string());
        let next = self.script.lock().unwrap().pop();
        match next {
            Some(Ok(content)) => Ok((content, Some(Usage { input_tokens: 10, output_tokens: 5 }))),
            Some(Err(())) | None => Err(crate::error::LlmError::Api {
                provider: "mock",
                status: 500,
                message: "hostile scripted response".into(),
            }),
        }
    }
}

fn source_track() -> Track {
    Track {
        lang: "tr".into(),
        cues: vec![
            Cue::new(0, 0, 2000, "merhaba dünya").unwrap(),
            Cue::new(1, 2500, 4500, "bu ikinci satır").unwrap(),
            Cue::new(2, 5000, 7000, "ve üçüncü").unwrap(),
        ],
        flags: vec![],
    }
}

fn opts<'a>(provider: &'a MockLlm) -> TranslateOptions<'a> {
    TranslateOptions {
        provider,
        model: "mock-model",
        target_lang: "de",
        user_prompt: "Halbsätze bevorzugen.",
        cache: None,
        brief: None,
        prior_context: false,
    }
}

fn opts_with_cache<'a>(provider: &'a MockLlm, cache: Option<&'a subtitler_store::Store>) -> TranslateOptions<'a> {
    TranslateOptions { cache, ..opts(provider) }
}

fn good_json(items: &[(u32, &str)]) -> String {
    let items: Vec<String> =
        items.iter().map(|(id, t)| format!(r#"{{"id": {id}, "text": "{}"}}"#, t)).collect();
    format!(r#"{{"translations": [{}]}}"#, items.join(", "))
}

#[tokio::test]
async fn happy_path_preserves_timecodes_and_ids() {
    let llm = MockLlm::with(vec![Ok(good_json(&[
        (0, "hallo welt"),
        (1, "dies ist zeile zwei"),
        (2, "und drei"),
    ]))]);
    let (out, _stats) = translate_track(&source_track(), opts(&llm)).await;
    assert_eq!(out.cues.len(), 3);
    assert_eq!(out.cues[0].text, "hallo welt");
    assert_eq!((out.cues[0].start_ms, out.cues[0].end_ms), (0, 2000));
    assert_eq!(out.cues[2].id, 2);
    assert!(out.flags.is_empty());
    assert_eq!(assert_timecodes_preserved(&source_track(), &out), Ok(()));
}

#[tokio::test]
async fn markdown_fences_are_unwrapped() {
    let llm = MockLlm::with(vec![Ok(format!(
        "```json\n{}\n```",
        good_json(&[(0, "hallo"), (1, "zwei"), (2, "drei")])
    ))]);
    let (out, _stats) = translate_track(&source_track(), opts(&llm)).await;
    assert_eq!(out.cues[0].text, "hallo");
    assert!(out.flags.is_empty());
}

#[tokio::test]
async fn invalid_batch_falls_back_to_single_cue_calls() {
    // Prose, then a missing id twice, then single-cue calls that succeed.
    let llm = MockLlm::with(vec![
        Ok(good_json(&[(2, "drei")])),
        Ok(good_json(&[(1, "eins")])),
        Ok(good_json(&[(0, "hallo")])),
        Ok(good_json(&[(0, "hallo"), (2, "drei")])), // missing id 1
        Ok(good_json(&[(0, "hallo"), (2, "drei")])),
        Ok("Here is your translation: {not json}".into()),
    ]);
    let (out, _stats) = translate_track(&source_track(), opts(&llm)).await;
    assert_eq!(out.cues.iter().filter(|c| c.text == "hallo").count(), 1);
    assert!(out.flags.iter().all(|f| matches!(f.flag, CueFlag::TranslationRetried)));
    assert_eq!(out.flags.len(), 3);
    assert_eq!(assert_timecodes_preserved(&source_track(), &out), Ok(()));
}

#[tokio::test]
async fn unrecoverable_cue_keeps_source_text_and_flag() {
    let mut script = vec![
        Ok("nope".into()),
        Ok("still nope".into()),
        Ok(r#"{"translations": [{"id": 0}, {"id": 1}, {"id": 2}]}"#.into()), // no text
    ];
    script.extend(std::iter::repeat_n(Ok("[".into()), 6));
    let llm = MockLlm::with(script);
    let (out, _stats) = translate_track(&source_track(), opts(&llm)).await;
    assert_eq!(out.cues[0].text, "merhaba dünya", "source text kept");
    assert_eq!(out.flags.len(), 3);
    assert!(out.flags.iter().all(|f| matches!(f.flag, CueFlag::TranslationFailed)));
    assert_eq!(assert_timecodes_preserved(&source_track(), &out), Ok(()));
}

#[tokio::test]
async fn validator_rejects_extra_missing_duplicate_and_empty() {
    let src = source_track();
    let batch: Vec<Cue> = src.cues.clone();

    let extra = validate_batch(
        &[0, 1, 2],
        &batch,
        &good_json(&[
            (0, "guten tag mein freund"),
            (1, "das ist die zweite zeile"),
            (9, "eine id die niemand bestellt hat"),
        ]),
    );
    assert!(extra.is_err(), "fazladan id reddedilmeli");

    let empty = validate_batch(
        &[0, 1, 2],
        &batch,
        &good_json(&[
            (0, "guten tag mein freund"),
            (1, "   "),
            (2, "und die dritte zeile"),
        ]),
    );
    assert!(empty.is_err(), "boş çıktı reddedilmeli");

    let dup = validate_batch(
        &[0, 1, 2],
        &batch,
        &good_json(&[
            (0, "guten tag mein freund"),
            (0, "guten tag nochmal"),
            (1, "das ist die zweite zeile"),
            (2, "und die dritte zeile"),
        ]),
    );
    assert!(dup.is_err(), "tekrarlanan id reddedilmeli");

    let line_break = validate_batch(
        &[0, 1, 2],
        &batch,
        &good_json(&[
            (0, "guten tag\nmein freund"),
            (1, "das ist die zweite zeile"),
            (2, "und die dritte zeile"),
        ]),
    );
    assert!(line_break.is_err(), "satır sonu reddedilmeli");

    let length = validate_batch(
        &[0, 1, 2],
        &batch,
        &good_json(&[
            (0, "guten tag mein freund"),
            (1, &"z".repeat(200)),
            (2, "und die dritte zeile"),
        ]),
    );
    assert!(length.is_err(), "oran bandı dışı reddedilmeli");

    let ok = validate_batch(
        &[0, 1, 2],
        &batch,
        &good_json(&[
            (0, "guten tag mein freund"),
            (1, "das ist die zweite zeile"),
            (2, "und die dritte zeile"),
        ]),
    );
    assert_eq!(ok.unwrap().len(), 3);
}

#[tokio::test]
async fn validator_rejects_a_cue_echoed_unchanged() {
    let batch = vec![Cue::new(0, 0, 2000, "Resmi gazetenin linkini vereceğim").unwrap()];
    let echoed = good_json(&[(0, "Resmi gazetenin linkini vereceğim")]);
    assert!(validate_batch(&[0], &batch, &echoed).is_err());

    let repunctuated = good_json(&[(0, "Resmi gazetenin  linkini vereceğim!")]);
    assert!(validate_batch(&[0], &batch, &repunctuated).is_err());

    let real = good_json(&[(0, "I will post the link from the official gazette")]);
    assert!(validate_batch(&[0], &batch, &real).is_ok());
}

#[tokio::test]
async fn validator_accepts_a_short_cue_left_unchanged() {
    for kept in ["download", "OK", "Ha?", "Mm-hmm."] {
        let batch = vec![Cue::new(0, 0, 2000, kept).unwrap()];
        let same = good_json(&[(0, kept)]);
        assert!(
            validate_batch(&[0], &batch, &same).is_ok(),
            "`{kept}` çevrilmeden kalabilmeli"
        );
    }
}

#[tokio::test]
async fn an_echoing_model_yields_translation_failed_flags() {
    let echo_all = good_json(&[
        (0, "merhaba dünya"),
        (1, "bu ikinci satır"),
        (2, "ve üçüncü"),
    ]);
    let llm = MockLlm::with(vec![
        Ok(good_json(&[(2, "ve üçüncü")])), // too short to judge → accepted
        Ok(good_json(&[(1, "bu ikinci satır")])),
        Ok(good_json(&[(1, "bu ikinci satır")])),
        Ok(good_json(&[(0, "merhaba dünya")])),
        Ok(good_json(&[(0, "merhaba dünya")])),
        Ok(echo_all.clone()),
        Ok(echo_all.clone()),
        Ok(echo_all),
    ]);
    let (out, _stats) = translate_track(&source_track(), opts(&llm)).await;

    assert_eq!(out.cues[0].text, "merhaba dünya", "echoed cue keeps its text");
    let flag = |cue_id: u32| {
        out.flags
            .iter()
            .find(|f| f.cue_id == cue_id)
            .map(|f| f.flag.clone())
    };
    assert!(matches!(flag(0), Some(CueFlag::TranslationFailed)));
    assert!(matches!(flag(1), Some(CueFlag::TranslationFailed)));
    assert!(matches!(flag(2), Some(CueFlag::TranslationRetried)));
    assert_eq!(assert_timecodes_preserved(&source_track(), &out), Ok(()));
}

#[tokio::test]
async fn a_rejected_echo_is_never_cached() {
    let dir = std::env::temp_dir().join(format!(
        "subtitler-translate-echo-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let store = subtitler_store::Store::open(&dir.join("store.db")).await.unwrap();

    let echo_all = good_json(&[
        (0, "merhaba dünya"),
        (1, "bu ikinci satır"),
        (2, "ve üçüncü"),
    ]);
    let llm = MockLlm::with(vec![
        Ok(good_json(&[(2, "und drei")])),
        Ok(good_json(&[(1, "eins")])),
        Ok(good_json(&[(0, "hallo")])),
        Ok(echo_all.clone()),
        Ok(echo_all.clone()),
        Ok(echo_all),
    ]);
    let (_t1, s1) = translate_track(&source_track(), opts_with_cache(&llm, Some(&store))).await;
    assert_eq!(s1.cache_hits, 0);

    // The batch key must miss (the echo wasn't cached); the singles hit.
    let llm2 = MockLlm::with(vec![]);
    let (t2, s2) = translate_track(&source_track(), opts_with_cache(&llm2, Some(&store))).await;
    assert_eq!(s2.cache_hits, 3);
    assert_eq!(
        t2.cues.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
        vec!["hallo", "eins", "und drei"]
    );
    assert!(t2.flags.iter().all(|f| matches!(f.flag, CueFlag::TranslationRetried)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn cache_serves_second_run_with_zero_llm_calls() {
    let dir = std::env::temp_dir().join(format!(
        "subtitler-translate-cache-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let store = subtitler_store::Store::open(&dir.join("store.db")).await.unwrap();

    let llm = MockLlm::with(vec![Ok(good_json(&[(0, "hallo"), (1, "zwei"), (2, "drei")]))]);
    let (t1, s1) = translate_track(&source_track(), opts_with_cache(&llm, Some(&store))).await;
    assert_eq!((s1.llm_calls, s1.cache_hits), (1, 0));

    // Empty script: any LLM call would fail, so everything must be cached.
    let llm2 = MockLlm::with(vec![]);
    let (t2, s2) = translate_track(&source_track(), opts_with_cache(&llm2, Some(&store))).await;
    assert_eq!((s2.llm_calls, s2.cache_hits), (0, 1));
    assert_eq!(t2.cues.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
               t1.cues.iter().map(|c| c.text.as_str()).collect::<Vec<_>>());
    let _ = std::fs::remove_dir_all(&dir);
}

fn context_of(request: &str, key: &str) -> Vec<String> {
    let parsed: serde_json::Value = serde_json::from_str(request).unwrap();
    parsed[key]
        .as_array()
        .expect(key)
        .iter()
        .map(|c| c["text"].as_str().expect("text").to_string())
        .collect()
}

#[tokio::test]
async fn the_single_cue_fallback_sends_its_neighbours() {
    // Three batch attempts, then one call per cue.
    let llm = MockLlm::with(vec![
        Ok(good_json(&[(2, "und drei")])),
        Ok(good_json(&[(1, "eins")])),
        Ok(good_json(&[(0, "hallo")])),
        Ok("not json".into()),
        Ok("not json".into()),
        Ok("not json".into()),
    ]);
    translate_track(&source_track(), opts(&llm)).await;

    let seen = llm.seen.lock().unwrap();
    let singles = &seen[3..];
    assert_eq!(singles.len(), 3, "her cue için tek istek atılmalı");

    for request in singles {
        assert_eq!(
            context_of(request, "translate").len(),
            1,
            "yalnızca tek satır çevrilmeli: {request}"
        );
    }
    assert!(context_of(&singles[0], "context_before").is_empty());
    assert_eq!(
        context_of(&singles[0], "context_after"),
        vec!["bu ikinci satır", "ve üçüncü"]
    );
    assert_eq!(context_of(&singles[1], "context_before"), vec!["merhaba dünya"]);
    assert_eq!(context_of(&singles[1], "context_after"), vec!["ve üçüncü"]);
    assert_eq!(
        context_of(&singles[2], "context_before"),
        vec!["merhaba dünya", "bu ikinci satır"]
    );
    assert!(context_of(&singles[2], "context_after").is_empty());
}

fn track(texts: &[&str]) -> Track {
    Track {
        lang: "en".into(),
        cues: texts
            .iter()
            .enumerate()
            .map(|(i, t)| Cue::new(i as u32, i as u32 * 1000, i as u32 * 1000 + 900, *t).unwrap())
            .collect(),
        flags: vec![],
    }
}

#[test]
fn sentence_groups_follow_punctuation_and_cap() {
    let t = track(&["It's raining", "cats and dogs.", "Yes!", "a", "b", "c", "d", "e."]);
    assert_eq!(sentence_groups(&t.cues), vec![0, 0, 1, 2, 2, 2, 2, 3]);
}

#[test]
fn group_length_band_allows_moving_words_inside_a_group() {
    let t = track(&["It's raining", "cats and dogs."]);
    let json = good_json(&[(0, "Es regnet in Strömen, wirklich"), (1, "ja")]);
    assert!(validate_batch(&[0, 1], &t.cues, &json).is_err());
    assert!(validate_grouped(&[0, 1], &t.cues, &[0, 0], &json).is_ok());
}

#[tokio::test]
async fn pro_mode_sends_groups_and_prior_translations() {
    let texts: Vec<String> = (0..25).map(|i| format!("line number {i} continues")).collect();
    let mut refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    refs[24] = "and ends here.";
    let t = track(&refs);
    let first: Vec<(u32, String)> = (0..20).map(|i| (i, format!("Zeile {i} geht weiter"))).collect();
    let second: Vec<(u32, String)> = (20..25).map(|i| (i, format!("Zeile {i} endet"))).collect();
    let to_json = |v: &[(u32, String)]| {
        good_json(&v.iter().map(|(i, s)| (*i, s.as_str())).collect::<Vec<_>>())
    };
    let llm = MockLlm::with(vec![Ok(to_json(&second)), Ok(to_json(&first))]);
    let brief = crate::Brief { media_type: "comedy".into(), ..Default::default() };
    let (out, _) = translate_track(&t, TranslateOptions { brief: Some(&brief), prior_context: true, ..opts(&llm) }).await;
    assert!(out.flags.is_empty());

    let seen = llm.seen.lock().unwrap();
    let req: serde_json::Value = serde_json::from_str(&seen[1]).unwrap();
    let before = req["context_before"].as_array().unwrap();
    assert_eq!(before.len(), 6);
    assert_eq!(before[5]["translation"], "Zeile 19 geht weiter");
    let first_req: serde_json::Value = serde_json::from_str(&seen[0]).unwrap();
    assert_eq!(first_req["translate"][0]["g"], 0);
}

#[tokio::test]
async fn failed_cues_are_not_offered_as_prior_translations() {
    let texts: Vec<String> = (0..21).map(|i| format!("line number {i} goes on")).collect();
    let t = track(&texts.iter().map(String::as_str).collect::<Vec<_>>());
    let first: Vec<(u32, String)> = (0..19).map(|i| (i, format!("Zeile {i} geht weiter"))).collect();
    let to_json = |v: &[(u32, String)]| {
        good_json(&v.iter().map(|(i, s)| (*i, s.as_str())).collect::<Vec<_>>())
    };
    // Batch 1 never returns id 19; its single-cue retries fail, batch 2 succeeds.
    let mut script = vec![Ok(good_json(&[(20, "Zeile 20")]))];
    script.push(Err(()));
    script.push(Err(()));
    for (i, s) in first.iter().rev() {
        script.push(Ok(good_json(&[(*i, s.as_str())])));
    }
    script.extend([Ok(to_json(&first)), Ok(to_json(&first)), Ok(to_json(&first))]);
    let llm = MockLlm::with(script);
    let (out, _) = translate_track(&t, TranslateOptions { prior_context: true, ..opts(&llm) }).await;
    assert!(out
        .flags
        .iter()
        .any(|f| f.cue_id == 19 && f.flag == CueFlag::TranslationFailed));

    let seen = llm.seen.lock().unwrap();
    let last: serde_json::Value = serde_json::from_str(seen.last().unwrap()).unwrap();
    let before = last["context_before"].as_array().unwrap();
    let cue19 = before.iter().find(|c| c["id"] == 19).unwrap();
    assert!(cue19.get("translation").is_none());
    assert_eq!(before.iter().find(|c| c["id"] == 18).unwrap()["translation"], "Zeile 18 geht weiter");
}
