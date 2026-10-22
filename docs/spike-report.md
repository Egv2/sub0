# Phase 0 — Spike report

> Status: COMPLETE ENOUGH TO EXIT (2026-10-13, `test-video.mp4`, 371 s Turkish
> speech). Raw artifacts in `docs/spike-data/` (gitignored): per-provider
> `words.json`, `segments.json`, `chunk_runs.json`, `spike-summary.md`.
>
> Re-run any time:
>
> ```sh
> set -a && source .env && set +a
> cargo run -p subtitler-cli -- spike --input test-video.mp4 --provider both
> ```

## Method

- `subtitler spike` runs ffprobe → full 16 kHz mono FLAC extraction →
  **conditional chunking** (see policy below) → per-provider transcription
  with `verbose_json` + word/segment granularities, temperature 0.
- Chunking policy (user decision): single request whenever possible; split at
  silence only when the FLAC exceeds the upload limit (default 25 MB,
  `--max-flac-mb`) **or** the duration exceeds the provider's single-request
  cap (Groq 600 s, OpenRouter 300 s). Chunk target default 600 s / cap 900 s
  (`--target-chunk-secs` / `--max-chunk-secs`), bounded by the provider cap.
- Measured per request: bytes, latency, word/segment counts, max
  `no_speech_prob`, mean `avg_logprob`; per provider: detected languages,
  top repeated trigram (repetition/hallucination indicator).

## Runs (test-video.mp4, 371 s)

| # | Mode | Groq turbo (words, latency) | OpenRouter large-v3 (words) | Wall time |
|---|---|---|---|---|
| 1 | force-chunked 180 s (old default) | 829 in 3 reqs, mean 6.1 s | 835 in 3 reqs, mean 19.4 s | ~2.5 min |
| 2 | single 371 s request (FLAC 12.6 MB) | **831 in 1 req, 27.9 s** | 0 words in 1 req, 44.9 s | 77 s |
| 3 | per-provider caps (600/300 s) | 831 in 1 req | 658 of ~843 in 2 reqs (75 s chunk returned **0 words**) | 55 s |
| 4 | OpenRouter only, 120 s chunks | — | 4/4 reqs with words, 843 total | ~2 min |
| 5 | new defaults, 2026-10-14: groq `whisper-large-v3` single + openrouter 2×300 s | 832 in 1 req, mean 9.9 s | 834 in 2 reqs, mean 12.8 s | ~2 min |

Run 5 also exercised profile segmentation on both providers' words:
**122 cues (groq) / 116 cues (openrouter)**, all ≤ 7 s — no more provider-sized
blocks; the remaining flags are dominated by `too_fast` (this speaker is fast;
threshold tuning is Phase 4, flags are surfaced per cue in `preview.html`).

Measured FLAC density: ≈ **2.1 MB/min** of dense speech → the 25 MB free-tier
limit corresponds to roughly **12 minutes**, not 30+. Keep the size check on
the extracted file, never a duration guess.

## Findings

1. **Both providers work end-to-end with one chunk format** (16 kHz mono
   FLAC). No 429s, no size-limit errors at any tested size.
2. **Word counts agree across providers and modes**: Groq 829–831, OpenRouter
   835–843 (Δ < 1%). Same top trigram in every run. Precise WER needs a
   human spot-check against the audio (pending).
3. **Groq honors word granularity up to at least a 371 s single request**
   (27.9 s round-trip, ~13× realtime incl. upload). Single-request cap
   default 600 s is a safe starting point [Verify at longer durations].
4. **OpenRouter word timestamps are unreliable on long/variable requests.**
   The upstream returned HTTP 200 with full segment text but an empty `words`
   array twice (371 s single; a 75 s chunk) while a 297 s chunk and all
   120 s chunks were fine. Hypothesis: per-request upstream routing, where
   some backends ignore `timestamp_granularities` — OpenRouter docs note
   routing preferences are not applied to transcriptions, so it cannot be
   pinned. Consequences:
   - keep OpenRouter's single-request cap at 300 s [Likely, tune];
   - Phase 1 must **detect the degraded shape** (words empty while word
     granularity was requested) and treat it as a retryable response;
   - default STT provider stays Groq; OpenRouter is the modularity/fallback
     option (still first-class for LLM use in Phase 2).
5. **Conditional chunking validated**: single-request mode cut wall time from
   ~2.5 min to ~55–77 s for a 6-minute video with identical word counts.
   Chunk boundaries stay clean (monotonic timestamps across the hard cut).
6. **Provider quirks normalized at the parse boundary** (`stt::verbose`):
   OpenRouter pads word/segment text with leading spaces (now trimmed, with a
   regression test); Groq returns a display language string ("Turkish"),
   OpenRouter an ISO code ("tr") — normalize in Phase 1 when persisting.
7. No hallucination indicators anywhere: 0 segments with
   `no_speech_prob > 0.9`; the 8 s music lead-in produced no phantom words.

## Post-spike finding (2026-10-14): hallucination metrics are not enough

The music-intro hallucination segment ("Altyazı ekleyen … çizgi film müziği")
carries **perfect** Whisper metrics (no_speech 0.005, avg_logprob −0.12,
compression 0.82 — better than real speech). §7.3 metric thresholds are
implemented (`stt::defenses`, config `DefenseThresholds`) and catch the common
classes, but confident music hallucinations require a real VAD (§4.1, Silero —
own task). Practical mitigations shipped: `--trim-start/end-secs` edge trim and
cleanup prompt v2 (tr), which instructs the cleanup LLM to drop non-sequitur
intro remnants — live-verified: phantom phrase removed from tr/en SRTs while
real content stayed intact.

## Remaining Phase 0 checklist

- [x] Groq + OpenRouter invocation verified against real APIs
- [x] Chunk sizing vs limits (max tested 12.6 MB; limit path logic tested)
- [x] Latency per stage measured
- [x] Conditional chunking policy implemented and validated
- [ ] Human WER spot-check of `docs/spike-data/*/words.json` vs audio
- [ ] yt-dlp sidecar + progress parsing (Phase 3 setup; yt-dlp not installed)
- [ ] Translation validator pass-rate on a 30-min sample (needs Phase 2)

## Exit

Architecture held under real API traffic; the only adjustment made during the
spike was conditional per-provider chunking (Sections above). Remaining open
items are either human-only (WER spot-check) or belong to later phases by
plan (yt-dlp → Phase 3, translation validator → Phase 2).
