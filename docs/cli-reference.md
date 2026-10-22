# CLI & Backend Parameter Reference

> Audience: future UI design (Tauri, Phase 5) and anyone scripting the CLI.
> Every UI option maps 1:1 to a CLI flag and a backend field — the UI holds no
> business logic. Last updated: 2026-10-14.

## Commands

| Command | Phase | What it does |
|---|---|---|
| `subtitler transcribe` | 1 | local file → segmented, profile-standard source-language SRT |
| `subtitler run` | 2 | transcribe + optional cleanup + translation into N languages |
| `subtitler spike` | 0 | STT measurement tool (providers side by side, artifacts + preview) |
| `subtitler llm-presets` | 2 | list curated cheap OpenRouter LLM presets |
| `subtitler batch` | 3 | persistent queue over mixed local files and URLs — sequential, resumable |

Exit codes: `0` success (warnings printed), `1` error (bad input, missing key,
failed stage). One failing cue never fails the job — it keeps source text and
gets a `translation_failed` flag (visible in preview + SRT tooling).

## Shared parameters (`transcribe` and `run`)

| CLI flag | Default | Backend field | UI notes |
|---|---|---|---|
| `--input <path>` | required | pipeline input | file picker; video or audio |
| `--provider <groq\|openrouter>` | `groq` | `SttSettings.provider` | segmented control; **STT provider is user-selectable** (modularity requirement) |
| `--stt-model <slug>` | provider default | `SttSettings.model` | dropdown per provider; groq default `whisper-large-v3` ($0.111/h, quality — user decision), alt `whisper-large-v3-turbo` ($0.04/h, fast); openrouter default `openai/whisper-large-v3` |
| `--language <iso>` | auto-detect | `SttSettings.language` | optional; detected language is recorded and normalized (Groq returns display names) |
| `--profile <path>` | built-in Netflix | `segment::Profile` | profile picker; TOML per §8 |
| `--preview` | off | renders `<out>/preview.html` | in-app preview later; today: open the HTML (video + every track, karaoke, flags, .vtt download) |
| `--cleanup` | **off** | `RunConfig.cleanup: Option<CleanupSetup>` | toggle "filler cleanup (experimental)"; needs an LLM provider |
| `--cleanup-prompt <path>` | auto per language | `CleanupSetup.prompt_text` | prompt editor; default resolves `prompts/cleanup.{lang}.md` → `cleanup.en.md` |
| `--cleanup-ratio <0..1>` | `0.2` | `CleanupSetup.max_deletion_ratio` | advanced slider; windows proposing more deletions are flagged and keep only non-lexical deletions |
| `--mode <fast\|pro>` | `pro` | `RunConfig.translation_mode` | "Çeviri kalitesi"; pro adds the localization brief, style profiles and translated context |
| `--no-rules` | off | `RunConfig.rule_cleanup = false` | skips deterministic caption/filler/loop removal |
| `--no-vad` | off | `SttSettings.vad.enabled = false` | skips Silero; chunking falls back to ffmpeg silencedetect, no word filter |
| `--max-flac-mb <n>` | `25` | chunk policy | advanced; chunking triggers only above this size (Groq free-tier upload limit) |
| `--target-chunk-secs <n>` | `600` | chunk policy | advanced; chunk target when chunking happens |
| `--max-chunk-secs <n>` | `900` | chunk policy | advanced; hard cap, bounded by the provider's single-request cap |
| `--noise-db <s>` | `-35dB` | silence detection | advanced; silencedetect threshold |
| `--min-silence-secs <f>` | `0.4` | silence detection | advanced; shortest pause considered a cut point |
| `--out <dir>` | `data/run` | output directory | save location |
| `--no-cache` | off | `RunConfig.cache_path` | advanced; disables the persistent store |
| `--trim-start-secs <n>` | `0` | edge trim (§7.3): drop words in the first N seconds — the practical fix for music-intro hallucinations |
| `--trim-end-secs <n>` | `0` | edge trim: drop words in the last N seconds (outro jingles) |

## `batch` parameters (Phase 3 queue, §10)

| CLI flag | Default | Notes |
|---|---|---|
| `--inputs <a,b,…>` | required (unless `--resume`) | local paths and/or URLs, mixed; URLs download via yt-dlp (audio-only) |
| `--resume <batch_id>` | — | continue an existing batch after a crash/cancel (Ctrl-C cancels cleanly; `kill_on_drop` reaps children) |
| `--retry-failed` | off | requeue the batch's failed jobs before draining (§10 retry); **requires `--resume`** (scopes to that batch). `attempts` and the stage checkpoint are preserved; error fields are cleared |
| `--prefetch-ahead <n>` | `2` | download lookahead (§10 `prefetch_ahead`): while a job processes, up to N upcoming queued URL jobs download concurrently into their job dirs; `0` disables |
| `--limit <n>` | — | process at most N queued jobs in this invocation |
| shared + `llm` flags | — | same flags as `run` (`--langs` optional: a batch without langs is transcribe-only and needs no LLM key) |

Semantics (SSOT): each input = one job row (SQLite `jobs`); one pasted list = one
`batches` row whose `settings_json` records the full `RunConfig`. **URL inputs are
expanded at enqueue time** (`yt-dlp --flat-playlist --print url`, §7.1): a playlist
becomes one url job per entry, a single video enqueues as itself; non-http inputs
are never expanded; exact-duplicate entries are dropped. Job dirs are
`<out>/batch_<id>/job_<n>/` with `n` sequential over ALL inputs after expansion
(`job_000..N`), so artifacts never collide across batches. Stage
checkpoints (`jobs.stage`, values from `core::JobStage`) + canonical artifacts
(`words.json`, `{lang}.track.json`) make reruns skip completed paid stages;
`jobs.status` lifecycle: queued → running → done / failed (`error_class`:
transient = retriable, permanent = give up; one failing job never stops the
queue). A job that fails on a transient download error can be brought back with
`--resume <id> --retry-failed`. Events broadcast as `core::JobEvent` (the future
Tauri stream). Queue tables: `batches`, `jobs`, `artifacts` (kind → path);
`store::list_jobs(batch)` is the UI/reporting projection.

Queue startup gates and prefetch:

- **yt-dlp version check**: `run_queue` prints `yt-dlp <version>` once and fails
  fast if the binary is missing (the queue cannot download). Auto-download of
  yt-dlp updates is deliberately **deferred to Phase 4** (§7.1 self-update).
- **Download prefetch** (§10, `--prefetch-ahead`, default 2): after a job is
  claimed, up to N upcoming queued URL jobs start downloading concurrently into
  their own job dirs. When a URL job's turn comes, its prefetch result is used
  if available, otherwise the download happens inline; a failed/absent prefetch
  falls back to the inline download, so failure classification and retries are
  identical to the unprefetched queue. Prefetch tasks are cancelled with the
  queue (`kill_on_drop` reaps yt-dlp). Wall-clock optimization only — `0` makes
  the queue strictly sequential.

## `run`-only parameters

| CLI flag | Default | Backend field | UI notes |
|---|---|---|---|
| `--langs <a,b,…>` | required | `RunConfig.langs` | multi-select of target languages; one SRT per language |
| `--llm-provider <groq\|openrouter>` | `groq` | `LlmSettings.provider` | same segmented control; cleanup reuses this provider |
| `--llm-preset <name>` | — | `LlmSettings.model` | preset dropdown (see below); mutually exclusive with `--llm-model` |
| `--llm-model <slug>` | groq: `openai/gpt-oss-120b` | `LlmSettings.model` | free slug entry; groq models verified via `GET /openai/v1/models` |
| `--prompt "<text>"` | empty | `RunConfig.user_translation_prompt` | textarea "style directives" appended to the system prompt |

### LLM presets (OpenRouter; `subtitler llm-presets`)

Slugs verified live 2026-10-14. All cheap + multilingual. **Prices fluctuate —
the UI must fetch live pricing per option when this becomes a picker.**

| Preset | Slug |
|---|---|
| `deepseek-v41-flash` | `deepseek/deepseek-v4.1-flash` |
| `deepseek-v4-flash` | `deepseek/deepseek-v4-flash-0731` |
| `glm-53-flash` | `z-ai/glm-5.3-flash` |
| `glm-47-flash` | `z-ai/glm-4.7-flash` |
| `gemma-4-26b` | `google/gemma-4-26b-a4b-it` |
| `qwen38-27b` | `qwen/qwen3.8-27b` |
| `qwen3-30b-a3b` | `qwen/qwen3-30b-a3b-instruct-2507` |
| `free-router` | `openrouter/free` |

Planned (noted by the user): local LLM support with **Qwen2.5-3B** behind the
same `LlmProvider` trait — add a "local" provider entry then.

### Provider behavior the UI should surface

- Groq free tier: **25 MB** upload cap (STT) and **200k tokens/day per LLM
  model** — near the cap the API may answer `200` with empty content; treat as
  retryable and warn about quota.
- OpenRouter STT: single requests stay reliable at **≤ 300 s** (caps are
  per-provider in the pipeline: groq 600 s, openrouter 300 s).
- Reasoning models (gpt-oss, some deepseek upstreams) need token headroom; the
  client already sends `max_tokens: 16384` and disables reasoning on
  OpenRouter.

## Profile TOML keys (`profiles/*.toml`)

| Key | Netflix default | Meaning |
|---|---|---|
| `name` | `"netflix"` | display name |
| `max_lines` | `2` | max subtitle lines per cue |
| `max_chars_per_line` | `42` | Latin-script default [Likely] |
| `max_cps` | `17.0` | reading speed (characters/second) |
| `min_duration_ms` | `833` | ~20 frames @ 24 fps; extended into silence |
| `max_duration_ms` | `7000` | hard cue length cap |
| `min_gap_ms` | `83` | 2 frames @ 24 fps |
| `pause_split_ms` | `500` | silence long enough to split a sentence [Verify] |

Per-language overrides stack on `[profile]` (Phase 4). Values are tracked with
sources in `docs/standards.md`; the segmenter flags residual violations
(`too_fast`, `too_long`, `too_short`, `line_overflow`) instead of silently
changing timing.

## Prompt files (versioned, part of the future cache key)

| File | Used by | Notes |
|---|---|---|
| `prompts/translate.system.md` | translation | `{{LOCALIZATION}}` (pro brief) and `{{USER_TRANSLATION_PROMPT}}` slots |
| `prompts/brief.system.md` | pro mode | localization brief, one call per source transcript |
| `prompts/styles/*.md` | pro mode | composable style profiles; `<!-- when: ... -->` header, loaded at runtime |
| `prompts/stt.{lang}.txt` | transcription | clean-style Whisper prompt; picked after the language probe |
| `prompts/cleanup.{lang}.md` | cleanup | fallback `cleanup.en.md`; `<!-- version: N -->` comment; v2 (tr) also removes music-intro hallucination phrases ("altyazı ekleyen", "çizgi film müziği") |

## Caching & usage (Phase 2 acceptance)

`data/store.db` (SQLite, WAL — §11) persists:

- `llm_cache` — keyed `blake3(task, prompt-version/hash, model, language, canonical input)`;
  unchanged prompts ⇒ **zero LLM calls** on rerun (measured: cold 40 s / 7 calls →
  warm 2.5 s / 0 calls). Changing `--prompt` or a prompt file invalidates exactly the
  affected entries.
- `stt_cache` — keyed over provider+model+language+temperature+audio hash; re-running
  the same video skips the STT API entirely.
- `usage` — per-call token accounting for cost reports (USD pricing lands in Phase 4).

The pipeline prints per stage: `stt cache: N/M chunk(s)`, `translation [x] cache:
N hit(s), M LLM call(s)`, `cleanup cache: …`.

## Environment

| Variable | Used for |
|---|---|
| `GROQ_API_KEY` | Groq STT and/or LLM |
| `OPENROUTER_API_KEY` | OpenRouter STT and/or LLM |

OS keychain (`keyring`) is the planned default (§12); env vars remain the
headless fallback.

## Output artifacts (per run directory)

| File | When |
|---|---|
| `{lang}.srt` | always (source track) |
| `{target}.srt` | one per `--langs` entry (timecodes byte-identical to source — enforced) |
| `words.json` | always; cleaned words when cleanup ran |
| `cleanup-deleted.json` | when cleanup ran; deleted word indices (audit/undo) |
| `speech.json` | Silero speech ranges |
| `vad-dropped.json` | words removed for falling outside speech |
| `rules-deleted.json` | words removed by the deterministic rules |
| `brief.json` | pro mode localization brief (hand-editable, reused on rerun) |
| `audio.flac` | always (16 kHz mono FLAC) |
| `chunk_XXXX.flac` | only when the conditional chunking policy split the audio |
| `preview.html` | with `--preview` |
| `data/store.db` | workspace-level (not per run): caches + usage + queue tables |
| `dropped-segments.json` | when §7.3 defenses removed anything (segments/words audit) |
| `source.track.json` / `{target}.track.json` | canonical Track artifacts (SRTs are views exported from these — resume re-exports without re-translating) |
| `meta.json` | detected/source language (`{"lang": "tr"}`) |
| `source.mp4`/`source.<ext>` | URL job downloads (yt-dlp) |

## Backend objects (UI design targets)

```
SttSettings { provider: groq|openrouter, model, language?, temperature: 0 }
LlmSettings { provider: groq|openrouter, model, temperature: 0 }
RunConfig   { stt, llm, langs[], user_translation_prompt, profile,
              max_flac_mb, target_chunk_secs, max_chunk_secs,
              stt_max_request_secs (provider-derived), noise_db,
              min_silence_secs, cleanup: Option<CleanupSetup>, prompts_dir }
CleanupSetup{ prompt_text?, max_deletion_ratio }
RunArtifacts{ dir, words, source Track, translations Track[], srt_paths }
```

## Invariants the UI must not break

1. Timecodes never pass through an LLM and never change in translation
   (export fails hard otherwise).
2. Cleanup only deletes words; it never rewrites text or shifts timestamps.
3. Cue boundaries are fixed at segmentation — translation cannot merge,
   split, drop, or reorder cues.
4. One failing job/cue never stops the queue (flags + source-text fallback).
