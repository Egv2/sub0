use subtitler_core::model::{CueFlag, Word};
use subtitler_core::time::secs_to_ms;
use unicode_segmentation::UnicodeSegmentation;

use crate::profile::Profile;
use crate::segmenter::segment;

fn w(text: &str, start_ms: u32, end_ms: u32) -> Word {
    Word {
        text: text.into(),
        start_ms,
        end_ms,
        prob: None,
    }
}

fn stream(sentence: &str, from_ms: u32) -> Vec<Word> {
    let mut words = Vec::new();
    let mut t = from_ms;
    for token in sentence.split_whitespace() {
        let dur = 360;
        words.push(w(token, t, t + dur));
        t += 625; // gap stays below the pause-split threshold
    }
    words
}

#[test]
fn splits_at_sentence_punctuation_and_respects_min_duration() {
    let sentence = "Bugün hava çok güzel. Dışarı çıkalım mı? Evet çıkalım.";
    let words = stream(sentence, 0);
    let track = segment(&words, &Profile::default(), "tr", None);

    assert!(
        track.cues.len() >= 2,
        "expected sentence splits, got {:?}",
        track.cues
    );
    for cue in &track.cues {
        assert!(cue.start_ms < cue.end_ms);
        assert!(cue.text.ends_with(['.', '?']) || !track_is_last_sentence(&track, cue));
    }
    // All words preserved exactly once, in order.
    let joined: String = track
        .cues
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        joined.split_whitespace().count(),
        sentence.split_whitespace().count()
    );
}

fn track_is_last_sentence(
    track: &subtitler_core::model::Track,
    cue: &subtitler_core::model::Cue,
) -> bool {
    track.cues.last().map(|c| c.id) == Some(cue.id)
}

#[test]
fn cues_never_overlap_and_keep_min_gap() {
    let mut text = String::new();
    for i in 0..40 {
        text.push_str(&format!("kelime{} ", i));
        if i % 6 == 5 {
            text.push_str("bitti. ");
        }
    }
    let words = stream(text.trim(), 100_000);
    let profile = Profile::default();
    let track = segment(&words, &profile, "tr", None);
    for pair in track.cues.windows(2) {
        assert!(pair[0].end_ms <= pair[1].start_ms, "overlap: {:?}", pair);
        assert!(pair[1].start_ms - pair[0].end_ms >= profile.min_gap_ms);
    }
    let mut last_end = 0;
    for cue in &track.cues {
        assert!(cue.start_ms >= last_end);
        last_end = cue.end_ms;
    }
}

#[test]
fn long_stream_is_capped_by_max_duration_and_chars() {
    let profile = Profile::default();
    let mut words = Vec::new();
    // ~50 s of period-free speech: only hard limits can split it.
    for round in 0..6 {
        words.extend(stream(
            "süper uzun bir cümle hiç nokta yok ve devam ediyor",
            round * 10_000,
        ));
    }
    let track = segment(&words, &profile, "tr", None);
    assert!(track.cues.len() >= 2, "got {} cues", track.cues.len());
    for cue in &track.cues {
        assert!(cue.duration_ms() <= profile.max_duration_ms, "{:?}", cue);
        assert!(
            cue.text.graphemes(true).count() <= profile.max_total_chars(),
            "{:?}",
            cue
        );
    }
}

#[test]
fn long_pause_splits_mid_sentence() {
    let mut words = stream("araya uzun bir sessizlik girse bile bölünmeli çünkü", 0);
    let last = words.last_mut().unwrap();
    last.start_ms += 900;
    last.end_ms += 900;
    let profile = Profile {
        pause_split_ms: 500,
        ..Profile::default()
    };
    let track = segment(&words, &profile, "tr", None);
    assert!(track.cues.len() >= 2);
    assert!(track.cues[0].text.ends_with("girse") || track.cues[0].text.ends_with("bölünmeli"));
}

#[test]
fn short_tail_cue_is_extended_into_silence_but_not_past_next() {
    // Short sentence followed by silence: split, then stretch to min duration.
    let words = vec![
        w("Evet.", 0, 400),
        w("Hemen", 2000, 2300),
        w("başlıyoruz.", 2300, 2700),
    ];
    let profile = Profile::default();
    let track = segment(&words, &profile, "tr", None);
    assert_eq!(track.cues[0].text, "Evet.");
    assert!(
        track.cues[0].duration_ms() >= profile.min_duration_ms,
        "extended: {:?}",
        track.cues[0]
    );
    assert!(
        track.cues[0].end_ms <= track.cues[1].start_ms - profile.min_gap_ms,
        "must not overlap next cue"
    );
}

#[test]
fn dense_short_cue_is_flagged_too_fast() {
    // One second cue holding ~40 chars → cps 40 > 17.
    let words = vec![w(
        "Bu epey uzun ve hızlı okunması gereken bir cümle parçası.",
        0,
        1000,
    )];
    let track = segment(&words, &Profile::default(), "tr", None);
    assert!(track
        .flags
        .iter()
        .any(|f| matches!(f.flag, CueFlag::TooFast { cps } if cps > 17.0)));
}

#[test]
fn frame_snapping_keeps_grid_and_order() {
    let words = stream(
        "Birinci cümle burada bitti. İkinci cümle de burada bitti.",
        0,
    );
    let track = segment(&words, &Profile::default(), "tr", Some(24.0));
    for cue in &track.cues {
        // Allow ±1 ms of float noise.
        let frame_ms = 1000.0 / 24.0;
        for t in [cue.start_ms, cue.end_ms] {
            let frames = t as f64 / frame_ms;
            assert!(
                (frames - frames.round()).abs() < 0.02,
                "{t} not on 24 fps grid"
            );
        }
    }
}

#[test]
fn empty_input_gives_empty_track() {
    let track = segment(&[], &Profile::default(), "en", None);
    assert!(track.cues.is_empty());
    assert_eq!(track.lang, "en");
}

#[test]
fn turbo_style_fragmented_words_still_produce_standard_cues() {
    // Whisper turbo produced a single 26 s segment here; the cap must still hold.
    let long_run = "burada sadece yaş bilgisi verilerek bu sistemin çalışabileceğini söylüyorlar veyetişkinler için de bir kez yaş doğrulaması yapmak yetecek gibi gözüküyor diyorlar";
    let mut words = Vec::new();
    let mut t = secs_to_ms(336.5);
    for token in long_run.split_whitespace() {
        words.push(w(token, t, t + 320));
        t += 700;
    }
    let track = segment(&words, &Profile::default(), "tr", None);
    assert!(track.cues.len() >= 2);
    for cue in &track.cues {
        assert!(
            cue.duration_ms() <= 7000,
            "26 s style block survived: {:?}",
            cue
        );
    }
}
