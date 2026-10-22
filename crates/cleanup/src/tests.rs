
use std::sync::Mutex;

use async_trait::async_trait;
use subtitler_core::model::Word;
use subtitler_translate::{LlmProvider, Usage};

use crate::{cleanup_words, CleanupOptions};

#[derive(Default)]
struct MockLlm {
    script: Mutex<Vec<String>>,
}

impl MockLlm {
    fn with(script: Vec<String>) -> Self {
        Self {
            script: Mutex::new(script),
        }
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
        _user: &str,
        _temperature: f32,
    ) -> Result<(String, Option<Usage>), subtitler_translate::LlmError> {
        let next = self.script.lock().unwrap().remove(0);
        Ok((next, None))
    }
}

fn words(n: usize) -> Vec<Word> {
    (0..n)
        .map(|i| Word {
            text: if i % 5 == 0 {
                "ıı".into()
            } else {
                format!("kelime{i}")
            },
            start_ms: (i * 400) as u32,
            end_ms: (i * 400 + 300) as u32,
            prob: None,
        })
        .collect()
}

fn opts<'a>(provider: &'a MockLlm, ratio: f32) -> CleanupOptions<'a> {
    CleanupOptions {
        provider,
        model: "mock",
        prompt: "delete fillers",
        lang: "en",
        max_deletion_ratio: ratio,
        cache: None,
    }
}

#[tokio::test]
async fn deletes_indices_and_preserves_timestamps() {
    let llm = MockLlm::with(vec![r#"{"delete": [0, 5, 10, 15]}"#.into()]);
    let out = cleanup_words(&words(20), &opts(&llm, 0.3)).await.unwrap();
    assert_eq!(out.deleted_indices, vec![0, 5, 10, 15]);
    assert_eq!(out.kept.len(), 16);
    assert!(out.flagged_windows.is_empty());
    assert_eq!((out.kept[0].start_ms, out.kept[0].end_ms), (400, 700));
    assert_eq!(out.kept[0].text, "kelime1");
}

#[tokio::test]
async fn out_of_range_index_rejects_window() {
    let llm = MockLlm::with(vec![r#"{"delete": [3, 999]}"#.into()]);
    let out = cleanup_words(&words(20), &opts(&llm, 0.3)).await.unwrap();
    assert!(out.deleted_indices.is_empty());
    assert_eq!(out.flagged_windows, vec![0]);
}

#[tokio::test]
async fn over_cap_window_keeps_only_nonlexical_deletions() {
    let mut w = words(10);
    w[2].text = "[sigh]".into();
    w[3].text = "*".into();
    w[7].text = "Mm".into();
    let llm = MockLlm::with(vec![r#"{"delete": [0,1,2,3,4,5,6,7,8,9]}"#.into()]);
    let out = cleanup_words(&w, &opts(&llm, 0.2)).await.unwrap();
    assert_eq!(out.deleted_indices, vec![2, 3, 7]);
    assert_eq!(out.flagged_windows, vec![0]);
}

#[tokio::test]
async fn deleting_nothing_is_fine() {
    let llm = MockLlm::with(vec![r#"{"delete": []}"#.into()]);
    let out = cleanup_words(&words(20), &opts(&llm, 0.3)).await.unwrap();
    assert!(out.deleted_indices.is_empty());
    assert_eq!(out.kept.len(), 20);
    assert!(out.flagged_windows.is_empty());
}

#[tokio::test]
async fn duplicate_indices_are_deduped() {
    let llm = MockLlm::with(vec![r#"{"delete": [5, 5, 5]}"#.into()]);
    let out = cleanup_words(&words(20), &opts(&llm, 0.3)).await.unwrap();
    assert_eq!(out.deleted_indices, vec![5]);
}

#[tokio::test]
async fn fences_are_stripped() {
    let llm = MockLlm::with(vec!["```json\n{\"delete\": [2]}\n```".into()]);
    let out = cleanup_words(&words(20), &opts(&llm, 0.3)).await.unwrap();
    assert_eq!(out.deleted_indices, vec![2]);
}

#[tokio::test]
async fn multiple_windows_are_processed_independently() {
    // WINDOW is 250, so a 30-word stream stays one window.
    let llm = MockLlm::with(vec![r#"{"delete": [0]}"#.into()]);
    let out = cleanup_words(&words(30), &opts(&llm, 0.3)).await.unwrap();
    assert_eq!(out.deleted_indices, vec![0]);
}
