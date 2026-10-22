//! A cue spans its first word's start to its last word's end. It only grows
//! into following silence to meet the minimum duration.

use std::collections::HashMap;

use subtitler_core::model::{Cue, CueFlag, Track, TrackFlag};
use unicode_segmentation::UnicodeSegmentation;

use crate::profile::Profile;

/// With `fps`, cue boundaries snap to the frame grid.
pub fn segment(
    words: &[subtitler_core::model::Word],
    profile: &Profile,
    lang: &str,
    fps: Option<f64>,
) -> Track {
    let max_total = profile.max_total_chars();
    let mut groups: Vec<(usize, usize)> = Vec::new(); // [first_word, last_word) ranges
    let mut start = 0usize;

    for i in 0..words.len() {
        let group_chars: usize = chars_of(&words[start..=i]);
        let next_gap = if i + 1 < words.len() {
            words[i + 1].start_ms.saturating_sub(words[i].end_ms)
        } else {
            u32::MAX
        };
        let sentence_end = ends_with_any(&words[i].text, &['.', '!', '?', '…', '؟']);
        let clause_end = ends_with_any(&words[i].text, &[',', ';', ':', '—']);

        // Measure the duration cap to the next word's *end*, since that's
        // where the cue would end if we took it.
        let hard_break = i + 1 == words.len()
            || next_word_would_exceed(words, start, i, max_total)
            || (i + 1 < words.len()
                && words[i + 1].end_ms.saturating_sub(words[start].start_ms)
                    > profile.max_duration_ms);
        // Only break at a sentence end if the cue is long enough or can be
        // extended into the silence after it. Otherwise merge forward.
        let want_break = (sentence_end
            && duration_of(words, start, i).saturating_add(next_gap) >= profile.min_duration_ms)
            || next_gap >= profile.pause_split_ms
            || (clause_end
                && group_chars >= max_total / 2
                && duration_of(words, start, i) >= profile.min_duration_ms);

        if hard_break || want_break {
            groups.push((start, i + 1));
            start = i + 1;
        }
    }

    let mut cues: Vec<Cue> = groups
        .iter()
        .enumerate()
        .map(|(id, (s, e))| Cue {
            id: id as u32,
            start_ms: words[*s].start_ms,
            end_ms: words[e - 1].end_ms,
            text: words[*s..*e]
                .iter()
                .map(|w| w.text.trim())
                .collect::<Vec<_>>()
                .join(" "),
        })
        .collect();

    extend_short_cues(&mut cues, profile);
    if let Some(fps) = fps {
        for cue in &mut cues {
            cue.start_ms = snap(cue.start_ms, fps);
            cue.end_ms = snap(cue.end_ms, fps);
        }
        // Snapping can reintroduce overlaps.
        for i in 0..cues.len().saturating_sub(1) {
            let limit = cues[i + 1].start_ms.saturating_sub(profile.min_gap_ms);
            if cues[i].end_ms > limit {
                cues[i].end_ms = limit.max(cues[i].start_ms + 1);
            }
        }
    }

    let mut flags_by_cue: HashMap<u32, Vec<CueFlag>> = HashMap::new();
    for cue in &cues {
        let dur = cue.duration_ms();
        let chars = cue.text.graphemes(true).count();
        if dur > profile.max_duration_ms {
            flags_by_cue
                .entry(cue.id)
                .or_default()
                .push(CueFlag::TooLong);
        }
        if (dur as usize) < profile.min_duration_ms as usize {
            flags_by_cue
                .entry(cue.id)
                .or_default()
                .push(CueFlag::TooShort);
        }
        if chars > max_total {
            flags_by_cue
                .entry(cue.id)
                .or_default()
                .push(CueFlag::LineOverflow);
        }
        let cps = chars as f32 / (dur as f32 / 1000.0).max(f32::EPSILON);
        if cps > profile.max_cps {
            flags_by_cue
                .entry(cue.id)
                .or_default()
                .push(CueFlag::TooFast { cps });
        }
    }

    let flags = cues
        .iter()
        .flat_map(|cue| {
            flags_by_cue
                .get(&cue.id)
                .into_iter()
                .flatten()
                .map(move |flag| TrackFlag {
                    cue_id: cue.id,
                    flag: flag.clone(),
                })
        })
        .collect();

    Track {
        lang: lang.to_string(),
        cues,
        flags,
    }
}

fn chars_of(words: &[subtitler_core::model::Word]) -> usize {
    join_words(words).graphemes(true).count()
}

fn join_words(words: &[subtitler_core::model::Word]) -> String {
    words
        .iter()
        .map(|w| w.text.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

fn duration_of(words: &[subtitler_core::model::Word], start: usize, end_incl: usize) -> u32 {
    words[end_incl].end_ms.saturating_sub(words[start].start_ms)
}

fn next_word_would_exceed(
    words: &[subtitler_core::model::Word],
    start: usize,
    end_incl: usize,
    max_total: usize,
) -> bool {
    let with_next = chars_of(&words[start..=end_incl])
        + 1
        + words[end_incl + 1].text.trim().graphemes(true).count();
    with_next > max_total
}

fn ends_with_any(text: &str, terminals: &[char]) -> bool {
    let trimmed = text.trim_end_matches(['"', '\'', ')', ']', '»', '”', ' ']);
    terminals.iter().any(|t| trimmed.ends_with(*t))
}

fn extend_short_cues(cues: &mut [Cue], profile: &Profile) {
    for i in 0..cues.len() {
        let dur = cues[i].duration_ms();
        if dur >= profile.min_duration_ms {
            continue;
        }
        let target_end = cues[i].start_ms + profile.min_duration_ms;
        let limit = if i + 1 < cues.len() {
            cues[i + 1].start_ms.saturating_sub(profile.min_gap_ms)
        } else {
            u32::MAX
        };
        cues[i].end_ms = cues[i]
            .end_ms
            .max(target_end.min(limit.max(cues[i].end_ms)));
    }
}

fn snap(ms: u32, fps: f64) -> u32 {
    let frames = ((ms as f64) * fps / 1000.0).round();
    ((frames * 1000.0 / fps).round()) as u32
}
