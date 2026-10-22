# Prompt files

Versioned, plain-text prompts. **The version line is part of every LLM cache
key** — bump it whenever you edit a file below and commit the change, so old
cache entries are invalidated exactly where needed.

- `cleanup.<lang>.md` — cleanup instructions selected by source language;
  the English prompt is the fallback and supports transcripts in any
  language. Both prompts cover non-speech sound captions, standalone listener
  murmurs, and language-dependent fillers. The LLM receives indexed words and
  may only return indices to delete.
- `translate.system.md` — translation system prompt (project rules + user
  preferences). The LLM receives `id → text` batches and must return one
  output per id, never touching timecodes, ids, order or count.
- `brief.system.md` — pro mode: one call per source transcript producing the
  language-agnostic localization brief (media type, register, audience,
  speakers, glossary, idioms). Shared by every target language.
- `styles/*.md` — composable style profiles. Each file starts with
  `<!-- when: ... -->`; the brief model sees that list and picks any that
  apply, and always writes a tailored `style_guide` for content no profile
  covers. Add a file to support a new kind of content, no code change needed.
  `profanity.md` is added automatically for strong language, never for kids.
- `stt.<lang>.txt` — Whisper `prompt`: a clean, normally punctuated subtitle
  sample. Whisper imitates its style, which suppresses fillers and sound
  captions. Sent only once the source language is known.
