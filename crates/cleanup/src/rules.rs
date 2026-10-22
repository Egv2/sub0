//! Deterministic, LLM-free cleanup: sound captions, fillers, murmurs, lone
//! interjections and Whisper repetition loops. Delete-only.

use subtitler_core::model::Word;

const MAX_CAPTION_WORDS: usize = 8;
// Real parentheticals ("(which was true)") are longer than sound captions.
const MAX_PAREN_CAPTION_WORDS: usize = 2;
/// Silence on both sides that makes an interjection a standalone utterance.
const ISOLATION_MS: u32 = 800;
const LOOP_MIN_UNIGRAM: usize = 6;
const LOOP_MIN_NGRAM: usize = 4;
const LOOP_MAX_N: usize = 4;
/// Repeats further apart than this are separate utterances, not a loop.
const LOOP_MAX_GAP_MS: u32 = 1000;

const UNIVERSAL_FILLERS: &[&str] = &["m", "hm", "mh", "mhm", "uhm", "erm", "uhuh", "mhmh"];
const LANG_FILLERS: &[(&str, &[&str])] = &[
    ("en", &["um", "uh", "er"]),
    ("tr", &["ı", "ıh", "ıhı", "hıhı", "hı", "e", "uh", "um"]),
    ("de", &["äh", "ähm", "öhm"]),
    ("fr", &["euh", "heu"]),
];
/// Single-letter forms are fillers only when drawn out ("eee", not "e").
const DRAWN_OUT_ONLY: &[&str] = &["e", "a", "o", "i"];
const DRAWN_OUT_MIN: usize = 3;
const INTERJECTIONS: &[&str] = &["ah", "oh", "eh", "uh", "ugh", "aw"];
// "ha" is a verb in Spanish/Portuguese, so it isn't universal.
const LANG_INTERJECTIONS: &[(&str, &[&str])] = &[
    ("en", &["ha", "hah"]),
    ("tr", &["of", "öf", "ay", "hıh"]),
];
const DISPOSABLE_SYMBOLS: &[char] = &[
    '*', '∗', '＊', '-', '–', '—', '…', '.', ',', '!', '?', ';', ':', '♪', '♫', '~', '_', '|',
    '[', ']', '(', ')', '"', '\'',
];

pub fn rule_deletions(words: &[Word], lang: &str) -> Vec<usize> {
    let mut deleted = vec![false; words.len()];
    mark_captions(words, &mut deleted);
    for (i, w) in words.iter().enumerate() {
        if !deleted[i] && is_symbol_only(&w.text) {
            deleted[i] = true;
        }
    }
    mark_murmurs_and_fillers(words, lang, &mut deleted);
    mark_isolated_interjections(words, lang, &mut deleted);
    mark_loops(words, &mut deleted);
    (0..words.len()).filter(|&i| deleted[i]).collect()
}

/// Used to salvage LLM deletions from a window that went over the ratio cap.
pub fn is_nonlexical(text: &str, lang: &str) -> bool {
    let t = text.trim();
    is_symbol_only(t) || is_single_caption(t) || is_filler(t, lang)
}

/// An opener has a letter right after the marker, so "****" (censored
/// words) never starts a caption.
fn opener(t: &str) -> Option<char> {
    if !t.chars().nth(1).is_some_and(char::is_alphabetic) {
        return None;
    }
    match t.chars().next()? {
        '[' => Some(']'),
        '(' => Some(')'),
        '*' | '∗' | '＊' => Some('*'),
        _ => None,
    }
}

fn closes(t: &str, closer: char) -> bool {
    let t = t.trim_end_matches(['.', ',', '!', '?', ';', ':', '"', '\'']);
    match closer {
        '*' => t.ends_with(['*', '∗', '＊']),
        c => t.ends_with(c),
    }
}

fn is_single_caption(t: &str) -> bool {
    opener(t).is_some_and(|closer| {
        let rest: String = t.chars().skip(1).collect();
        rest.chars().any(char::is_alphabetic) && closes(&rest, closer)
    })
}

fn mark_captions(words: &[Word], deleted: &mut [bool]) {
    let mut i = 0;
    while i < words.len() {
        let t = words[i].text.trim();
        if let Some(closer) = opener(t) {
            let max = if closer == ')' { MAX_PAREN_CAPTION_WORDS } else { MAX_CAPTION_WORDS };
            let end = if is_single_caption(t) {
                Some(i)
            } else {
                (i + 1..words.len().min(i + max))
                    .find(|&j| closes(words[j].text.trim(), closer))
            };
            if let Some(end) = end {
                deleted[i..=end].iter_mut().for_each(|d| *d = true);
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
}

fn is_symbol_only(t: &str) -> bool {
    let t = t.trim();
    !t.is_empty() && t.chars().all(|c| DISPOSABLE_SYMBOLS.contains(&c))
}

/// Letters only, ignoring edge punctuation and inner hyphens ("Mm-hmm.").
fn is_wordlike(t: &str) -> bool {
    let core = t.trim_matches(|c: char| !c.is_alphanumeric());
    !core.is_empty() && core.chars().all(|c| c.is_alphabetic() || c == '-')
}

fn letters(t: &str) -> String {
    t.chars()
        .filter(|c| c.is_alphabetic())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = None;
    for c in s.chars() {
        if Some(c) != last {
            out.push(c);
        }
        last = Some(c);
    }
    out
}

fn base_lang(lang: &str) -> &str {
    lang.split(['-', '_']).next().unwrap_or(lang)
}

fn is_filler(t: &str, lang: &str) -> bool {
    if !is_wordlike(t) {
        return false;
    }
    let raw = letters(t);
    // A lone "M" is an initial, not a hum.
    if raw == "m" {
        return false;
    }
    let c = collapse(&raw);
    let lang = base_lang(lang);
    let in_lang = LANG_FILLERS
        .iter()
        .filter(|(l, _)| *l == lang)
        .any(|(_, set)| set.contains(&c.as_str()));
    if !(UNIVERSAL_FILLERS.contains(&c.as_str()) || in_lang) {
        return false;
    }
    !DRAWN_OUT_ONLY.contains(&c.as_str()) || raw.chars().count() >= DRAWN_OUT_MIN
}

fn is_interjection(t: &str, lang: &str) -> bool {
    let c = collapse(&letters(t));
    let lang = base_lang(lang);
    is_filler(t, lang)
        || INTERJECTIONS.contains(&c.as_str())
        || LANG_INTERJECTIONS
            .iter()
            .any(|(l, set)| *l == lang && set.contains(&c.as_str()))
        || is_laughter(&c)
}

fn is_laughter(c: &str) -> bool {
    let body = c.strip_suffix('h').unwrap_or(c);
    body.len() >= 4 && body.as_bytes().chunks(2).all(|p| p == b"ha")
}

/// "Mm -hmm" / "Uh -huh": Whisper splits murmurs at the hyphen.
fn mark_murmurs_and_fillers(words: &[Word], lang: &str, deleted: &mut [bool]) {
    for i in 0..words.len() {
        if deleted[i] {
            continue;
        }
        let t = words[i].text.trim();
        // "10 mm" is a unit.
        if i > 0 && words[i - 1].text.trim_end().ends_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        if i + 1 < words.len() && words[i + 1].text.trim_start().starts_with('-') {
            let joined = format!("{t}{}", words[i + 1].text.trim());
            if is_filler(&joined, lang) {
                deleted[i] = true;
                deleted[i + 1] = true;
                continue;
            }
        }
        if is_filler(t, lang) {
            deleted[i] = true;
        }
    }
}

fn mark_isolated_interjections(words: &[Word], lang: &str, deleted: &mut [bool]) {
    let mut i = 0;
    while i < words.len() {
        if deleted[i] || !is_interjection(&words[i].text, lang) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j + 1 < words.len() && (deleted[j + 1] || is_interjection(&words[j + 1].text, lang)) {
            j += 1;
        }
        let prev_end = (0..i).rev().find(|&k| !deleted[k]).map(|k| words[k].end_ms);
        let next_start = (j + 1..words.len()).find(|&k| !deleted[k]).map(|k| words[k].start_ms);
        let quiet_before = prev_end.is_none_or(|e| words[i].start_ms.saturating_sub(e) >= ISOLATION_MS);
        let quiet_after = next_start.is_none_or(|s| s.saturating_sub(words[j].end_ms) >= ISOLATION_MS);
        if quiet_before && quiet_after {
            deleted[i..=j].iter_mut().for_each(|d| *d = true);
        }
        i = j + 1;
    }
}

fn mark_loops(words: &[Word], deleted: &mut [bool]) {
    let live: Vec<usize> = (0..words.len()).filter(|&i| !deleted[i]).collect();
    let norm: Vec<String> = live
        .iter()
        .map(|&i| {
            words[i]
                .text
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect()
        })
        .collect();
    let mut drop = vec![false; live.len()];
    for n in 1..=LOOP_MAX_N {
        let min = if n == 1 { LOOP_MIN_UNIGRAM } else { LOOP_MIN_NGRAM };
        let mut s = 0;
        while s + n <= live.len() {
            if norm[s..s + n].iter().any(String::is_empty) {
                s += 1;
                continue;
            }
            let mut reps = 1;
            while s + (reps + 1) * n <= live.len()
                && norm[s + reps * n..s + (reps + 1) * n] == norm[s..s + n]
                && (s + reps * n - 1..s + (reps + 1) * n - 1).all(|k| {
                    words[live[k + 1]].start_ms.saturating_sub(words[live[k]].end_ms)
                        <= LOOP_MAX_GAP_MS
                })
            {
                reps += 1;
            }
            if reps >= min {
                drop[s + n..s + reps * n].iter_mut().for_each(|d| *d = true);
                s += reps * n;
            } else {
                s += 1;
            }
        }
    }
    for (k, &i) in live.iter().enumerate() {
        if drop[k] {
            deleted[i] = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words spaced 300 ms apart; `|` inserts a 2 s pause.
    fn words(script: &str) -> Vec<Word> {
        let mut t = 0;
        let mut out = Vec::new();
        for tok in script.split(' ') {
            if tok == "|" {
                t += 2000;
                continue;
            }
            out.push(Word {
                text: tok.into(),
                start_ms: t,
                end_ms: t + 250,
                prob: None,
            });
            t += 300;
        }
        out
    }

    fn kept(script: &str, lang: &str) -> String {
        let w = words(script);
        let del = rule_deletions(&w, lang);
        w.iter()
            .enumerate()
            .filter(|(i, _)| !del.contains(i))
            .map(|(_, w)| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn split_star_captions_are_removed() {
        assert_eq!(kept("*whisper * *whisper * *whisper *", "en"), "");
        assert_eq!(kept("*laughs * Ugh.", "en"), "");
        assert_eq!(kept("∗Sigh *", "en"), "");
        assert_eq!(kept("I [door slams] heard that.", "en"), "I heard that.");
        assert_eq!(kept("(laughter) So funny.", "en"), "So funny.");
    }

    #[test]
    fn murmurs_and_drawn_out_hums_go() {
        assert_eq!(kept("Mmmmmmmmmmmmmmmmmmmm", "en"), "");
        assert_eq!(kept("Look. Uh -huh.", "en"), "Look.");
        assert_eq!(kept("Mm -hmm.", "en"), "");
        assert_eq!(kept("I, um, think so.", "en"), "I, think so.");
        assert_eq!(kept("Iıı bence ııı olur.", "tr"), "Iıı bence olur.");
        assert_eq!(kept("Eee ne yapalım?", "tr"), "ne yapalım?");
        assert_eq!(kept("E ne yapalım?", "tr"), "E ne yapalım?");
    }

    #[test]
    fn isolated_interjections_go_but_meaningful_ones_stay() {
        assert_eq!(kept("We left. | Ah. | Then we came back.", "en"), "We left. Then we came back.");
        assert_eq!(kept("| Oh. Mm -hmm. |", "en"), "");
        assert_eq!(kept("Oh, oh, excuse me.", "en"), "Oh, oh, excuse me.");
        assert_eq!(kept("Ah, now I get it.", "en"), "Ah, now I get it.");
        assert_eq!(kept("Hahaha | right.", "en"), "right.");
    }

    #[test]
    fn language_sensitive_fillers_are_not_universal() {
        assert_eq!(kept("Um acht Uhr.", "de"), "Um acht Uhr.");
        assert_eq!(kept("Er ist da.", "de"), "Er ist da.");
        assert_eq!(kept("Äh, ich weiß nicht.", "de"), "ich weiß nicht.");
    }

    #[test]
    fn repetition_loops_collapse_to_one() {
        assert_eq!(kept("thank you thank you thank you thank you thank you", "en"), "thank you");
        assert_eq!(kept("no no no", "en"), "no no no");
        assert_eq!(kept("go go go go go go go", "en"), "go");
    }

    #[test]
    fn units_money_and_censoring_survive() {
        assert_eq!(kept("It is 10 mm wide.", "en"), "It is 10 mm wide.");
        assert_eq!(kept("Raised $5M from M&M fans.", "en"), "Raised $5M from M&M fans.");
        assert_eq!(kept("Michael M. Smith", "en"), "Michael M. Smith");
        assert_eq!(kept("100 ₺ and 20 ° # @", "tr"), "100 ₺ and 20 ° # @");
        assert_eq!(kept("What the **** is this ****", "en"), "What the is this");
        assert_eq!(kept("He said (which was true) nothing.", "en"), "He said (which was true) nothing.");
        assert_eq!(kept("¡Ha llegado! | Ha. |", "es"), "¡Ha llegado! Ha.");
    }

    #[test]
    fn spaced_out_repeats_are_not_a_loop() {
        assert_eq!(
            kept("Thank you. | Thank you. | Thank you. | Thank you. | Thank you. | Thank you.", "en"),
            "Thank you. Thank you. Thank you. Thank you. Thank you. Thank you."
        );
    }

    #[test]
    fn real_words_and_symbols_survive() {
        assert_eq!(kept("50 % off & more", "en"), "50 % off & more");
        assert_eq!(kept("Clap your hands.", "en"), "Clap your hands.");
        assert_eq!(kept("The * key", "en"), "The key");
    }
}

#[cfg(test)]
mod smoke {
    /// `SUBTITLER_RULES_WORDS=<words.json> SUBTITLER_RULES_LANG=tr cargo test -p subtitler-cleanup rules_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rules_smoke() {
        let path = std::env::var("SUBTITLER_RULES_WORDS").expect("SUBTITLER_RULES_WORDS");
        let lang = std::env::var("SUBTITLER_RULES_LANG").unwrap_or_else(|_| "en".into());
        let words: Vec<subtitler_core::model::Word> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        for i in super::rule_deletions(&words, &lang) {
            let ctx: Vec<&str> = words[i.saturating_sub(3)..(i + 4).min(words.len())]
                .iter()
                .map(|w| w.text.as_str())
                .collect();
            println!("{i:>5} {:<12} | {}", words[i].text, ctx.join(" "));
        }
    }
}
