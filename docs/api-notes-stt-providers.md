# Provider API notes — verified 2026-10-13

Findings from the official docs at implementation time.
Re-verify before changing anything below. Both platforms expose **LLMs and
Whisper-class STT**, so the STT provider is user-selectable between them.

## Groq

Source: <https://console.groq.com/docs/speech-to-text> (+ model pages).

- Endpoint: `POST https://api.groq.com/openai/v1/audio/transcriptions`
  (translations: `.../audio/translations`, target `en` only).
- Auth: `Authorization: Bearer $GROQ_API_KEY`.
- Request (multipart): `file`, `model`, `language` (ISO-639-1), `prompt`
  (max **224 tokens**, style/spelling hint), `response_format`
  (`json|verbose_json|text`), `temperature` (0–1, default 0),
  `timestamp_granularities[]` (repeat for `word` and `segment`;
  **requires** `response_format=verbose_json`).
- `verbose_json` segments: `id, seek, start, end, text, tokens, temperature,
  avg_logprob, compression_ratio, no_speech_prob`. With word granularity the
  `words` array carries only `word, start, end` (seconds, floats).
- Models: `whisper-large-v3` ($0.111/h audio, WER 10.3%, +translation),
  `whisper-large-v3-turbo` ($0.04/h, WER 12%, transcription only).
- Limits: file ≤ **25 MB free tier / 100 MB dev tier**; min length 0.01 s;
  minimum billed length 10 s; audio downsampled server-side to 16 kHz mono;
  only the first audio track is transcribed. Formats: flac, mp3, mp4, mpeg,
  mpga, m4a, ogg, wav, webm.

## OpenRouter

Sources: <https://openrouter.ai/docs/guides/overview/multimodal/stt>,
<https://openrouter.ai/docs/guides/overview/multimodal/audio>.

- Endpoint: `POST https://openrouter.ai/api/v1/audio/transcriptions`
  (OpenAI-compatible; also usable from OpenAI SDKs via `base_url`).
- Auth: `Authorization: Bearer $OPENROUTER_API_KEY`.
- Request forms:
  1. JSON with base64: `{"model", "input_audio": {"data": "<base64, not a data URI>", "format": "wav|mp3|flac|m4a|ogg|webm|aac"}, "language", "temperature", "response_format", "timestamp_granularities": ["segment","word"]}`
  2. OpenAI-style multipart (`file`, `model`, …) — capped at **25 MB**;
     `prompt` accepted but ignored; `text`/`srt`/`vtt` response formats → 400.
- `verbose_json` adds `task, language, duration, confidence, segments
  [{start, end, text, speaker?}], words [{word, start, end, confidence?,
  speaker?}]`. Word timestamps require `response_format=verbose_json`.
- Model slugs seen in docs: `openai/whisper-large-v3`, `openai/whisper-1`,
  `openai/gpt-4o-transcribe`, `microsoft/mai-transcribe-*`. **Caveat:** some
  (e.g. `gpt-4o-transcribe`) reject `verbose_json` → no word timestamps; do
  not use them as STT defaults. Discover via
  `GET /api/v1/models?output_modalities=transcription`.
- Limits: upstream provider timeout **60 s per request** → chunk durations
  must stay comfortably below ~a minute of provider processing time (our
  silence-based chunk planner covers this). Base64 JSON may exceed the 25 MB
  multipart cap on Groq/OpenAI-backed models; other backends reject it.
- Routing: `provider` routing preferences are not applied to transcriptions.
- ⚠️ **Measured 2026-10-13** (docs/spike-report.md): some upstreams return
  HTTP 200 with correct segment text but an **empty `words` array** even when
  word granularity was requested — observed on a 371 s single request and a
  75 s chunk, not on ≤ 300 s chunks. Treat "words empty while word
  granularity was requested" as a retryable degraded response, and keep the
  single-request duration cap conservative (spike default: 300 s).

## LLM (chat completions) — for cleanup (§7.4) and translation (§9)

Both platforms are OpenAI-compatible chat-completions providers; the plan's
assumption (§4.1) holds.

- Groq: `POST https://api.groq.com/openai/v1/chat/completions`.
- OpenRouter: `POST https://openrouter.ai/api/v1/chat/completions`
  (model slugs like `vendor/model`; supports routing/fallback options).

Verified live 2026-10-13/14 (`GET /openai/v1/models`, `GET /api/v1/models`):

- **Groq chat models (2026-10-14)**: `openai/gpt-oss-120b`, `openai/gpt-oss-20b`,
  `qwen/qwen3.8-27b`, `allam-2-7b` (+ prompt-guard, Orpheus TTS models).
  `llama-3.3-70b-versatile` is **retired** (404 model_not_found).
- **Groq free tier**: **200 000 tokens/day (TPD) per model**. Near the cap the
  API may answer `200` with **empty `choices[0].message.content`** instead of a
  clean 429 (observed 2026-10-13) — the translate ladder must treat empty
  content as retryable and the docs must warn about quota-burn loops.
  429 bodies carry `type: "tokens"` with an explicit retry window.
- **gpt-oss models are reasoning models**: hidden reasoning consumes the
  completion budget *before* `content`. Send `max_tokens ≥ 8192` and keep
  batches small (we use 20 cues; §9.1's 30–50 starved the content field).
- **OpenRouter cheap/fast candidates (2026-10-14)**: `deepseek/deepseek-v4.1-flash`
  ($0.04/$0.77 per Mtok, 1 M ctx), `openai/gpt-oss-120b` ($0.15/$0.60),
  `google/gemma-4-31b-it:free` (free but heavily throttled — an e2e run
  stalled; prefer paid-cheap for interactive runs).

Consequences for the pipeline:

1. Chunk default target 600 s / cap 900 s with the conditional policy; the
   OpenRouter STT single-request cap stays 300 s (see STT section above).
2. Both providers return seconds as floats — the only float→`u32` ms boundary
   (`core::time::secs_to_ms`), per invariant 3.
3. `prompt` hint works on Groq only; the OpenRouter path must not rely on it.
4. Word `confidence` is OpenRouter-only; Groq words have no probability →
   `Word.prob = None`.
5. Groq STT reports language as a display name ("Turkish"), OpenRouter as an
   ISO code ("tr") — `pipeline::normalize_lang_code` maps common names.

