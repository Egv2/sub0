//! Whisper hallucination filters. Everything dropped ends up in the report.
//!
//! Hallucinations over music can have perfect confidence metrics, so these
//! filters alone won't catch them; that needs VAD or manual trimming.

use subtitler_core::config::DefenseThresholds;
use subtitler_core::model::Word;

use crate::types::Segment;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct DefenseReport {
    pub dropped_segments: Vec<Segment>,
    pub dropped_words: Vec<Word>,
    pub collapsed_segments: usize,
}

pub fn apply_defenses(
    words: &[Word],
    segments: &[Segment],
    thresholds: &DefenseThresholds,
    trim: &TrimRange,
) -> (Vec<Word>, Vec<Segment>, DefenseReport) {
    let mut report = DefenseReport::default();

    let mut keep = vec![true; segments.len()];
    for (i, seg) in segments.iter().enumerate() {
        let fails = seg.no_speech_prob > thresholds.max_no_speech_prob
            || seg.avg_logprob < thresholds.min_avg_logprob
            || seg.compression_ratio > thresholds.max_compression_ratio;
        if fails {
            keep[i] = false;
            report.dropped_segments.push(seg.clone());
        }
    }

    if thresholds.collapse_repeated {
        let mut last: Option<String> = None;
        for (i, seg) in segments.iter().enumerate() {
            let norm = seg.text.trim().to_lowercase();
            if !norm.is_empty() && last.as_deref() == Some(norm.as_str()) && keep[i] {
                keep[i] = false;
                report.collapsed_segments += 1;
            }
            if keep[i] {
                last = Some(norm);
            }
        }
    }

    let in_trim = |ms: u32| ms >= trim.start_ms && ms <= trim.end_ms;
    let mut kept_words = Vec::with_capacity(words.len());
    for word in words {
        let mid = (word.start_ms.saturating_add(word.end_ms)) / 2;
        let covered = segments
            .iter()
            .zip(&keep)
            .any(|(seg, k)| *k && word.start_ms >= seg.start_ms && word.start_ms < seg.end_ms);
        // Words outside any segment are kept: providers leave gaps on short files.
        let in_dropped = segments
            .iter()
            .zip(&keep)
            .any(|(seg, k)| !*k && word.start_ms >= seg.start_ms && word.start_ms < seg.end_ms);
        if in_trim(mid)
            && !in_dropped
            && (covered
                || !segments
                    .iter()
                    .any(|seg| word.start_ms >= seg.start_ms && word.start_ms < seg.end_ms))
        {
            kept_words.push(word.clone());
        } else {
            report.dropped_words.push(word.clone());
        }
    }

    let kept_segments: Vec<Segment> = segments
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(seg, _)| seg.clone())
        .collect();

    (kept_words, kept_segments, report)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimRange {
    pub start_ms: u32,
    pub end_ms: u32,
}

impl TrimRange {
    pub fn new(start_ms: u32, end_ms: Option<u32>) -> Self {
        Self {
            start_ms,
            end_ms: end_ms.unwrap_or(u32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, start: u32, end: u32) -> Word {
        Word {
            text: text.into(),
            start_ms: start,
            end_ms: end,
            prob: None,
        }
    }

    fn seg(start: u32, end: u32, text: &str, no_speech: f32, logprob: f32, compr: f32) -> Segment {
        Segment {
            start_ms: start,
            end_ms: end,
            text: text.into(),
            no_speech_prob: no_speech,
            avg_logprob: logprob,
            compression_ratio: compr,
        }
    }

    #[test]
    fn drops_high_no_speech_segments_with_words() {
        let words = vec![word("gerçek", 1000, 1200), word("hayal", 5000, 5200)];
        let segments = vec![
            seg(0, 2000, "gerçek konuşma", 0.01, -0.2, 1.2),
            seg(4000, 6000, "halüsinasyon", 0.9, -0.3, 1.1),
        ];
        let (w, s, report) = apply_defenses(
            &words,
            &segments,
            &DefenseThresholds::default(),
            &TrimRange::new(0, None),
        );
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].text, "gerçek");
        assert_eq!(s.len(), 1);
        assert_eq!(report.dropped_segments.len(), 1);
        assert_eq!(report.dropped_words.len(), 1);
    }

    #[test]
    fn drops_repetition_loops() {
        let words = vec![word("tekrar", 1000, 1100)];
        let segments = vec![seg(0, 2000, "normal", 0.0, -0.2, 1.2)];
        let looping = vec![seg(
            3000,
            8000,
            "ve sonra ve sonra ve sonra",
            0.0,
            -0.2,
            5.1,
        )];
        let (w, _, report) = apply_defenses(
            &words,
            &[segments, looping].concat(),
            &DefenseThresholds::default(),
            &TrimRange::new(0, None),
        );
        assert_eq!(w.len(), 1, "kept words survive the loop drop");
        assert_eq!(report.dropped_segments.len(), 1);
    }

    #[test]
    fn collapses_consecutive_identical_segments() {
        let words = vec![
            word("a", 0, 100),
            word("b", 100, 200),
            word("a", 5000, 5100),
            word("b", 5100, 5200),
        ];
        let segments = vec![
            seg(0, 2000, "Aynı cümle.", 0.0, -0.2, 1.1),
            seg(5000, 7000, "aynı cümle.", 0.0, -0.2, 1.1),
        ];
        let (w, s, report) = apply_defenses(
            &words,
            &segments,
            &DefenseThresholds::default(),
            &TrimRange::new(0, None),
        );
        assert_eq!(s.len(), 1);
        assert_eq!(report.collapsed_segments, 1);
        assert_eq!(
            w.iter().filter(|x| x.text == "a").count(),
            1,
            "second copy's words dropped"
        );
    }

    #[test]
    fn trim_removes_edge_words_only() {
        let words = vec![word("jenerik", 100, 300), word("konuşma", 40_000, 40_200)];
        let segments = vec![
            seg(0, 1000, "jenerik", 0.0, -0.2, 1.0),
            seg(39_000, 41_000, "konuşma", 0.0, -0.2, 1.0),
        ];
        let (w, _, report) = apply_defenses(
            &words,
            &segments,
            &DefenseThresholds::default(),
            &TrimRange::new(29_000, None),
        );
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].text, "konuşma");
        assert_eq!(report.dropped_words.len(), 1);
    }

    #[test]
    fn perfect_metrics_survive_conservative_defaults() {
        // Real case: a music hallucination with clean metrics. Known limit.
        let words = vec![word("müzik", 1000, 1200)];
        let segments = vec![seg(0, 30_000, "müzik üstüne metin", 0.005, -0.12, 0.82)];
        let (w, s, report) = apply_defenses(
            &words,
            &segments,
            &DefenseThresholds::default(),
            &TrimRange::new(0, None),
        );
        assert_eq!((w.len(), s.len(), report.dropped_segments.len()), (1, 1, 0));
    }
}
