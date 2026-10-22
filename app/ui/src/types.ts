// Mirrors of the Rust types as serde serialises them. Rust is the source of truth.

export type JobStage =
  | "queued"
  | "downloading"
  | "extracting"
  | "chunking"
  | "transcribing"
  | "cleaning"
  | "segmenting"
  | "translating"
  | "validating"
  | "exporting"
  | "done"
  | "failed"
  | "cancelled";

export type JobStatus = "queued" | "running" | "done" | "failed" | "cancelled";

/** store::JobRow. */
export interface JobRow {
  id: number;
  batch_id: number;
  source_kind: "url" | "file";
  source_ref: string;
  stage: JobStage;
  status: JobStatus;
  error_class: string | null;
  error_msg: string | null;
  attempts: number;
  out_dir: string;
}

/** store::ArtifactRow. */
export interface ArtifactRow {
  kind: "source" | "words" | "track" | "srt" | string;
  path: string;
  /** Empty for the source transcript. */
  lang: string;
}

/** subtitler_core::lang::Language, a selectable target language. */
export interface LanguageOption {
  code: string;
  /** Turkish name. */
  name: string;
  /** Endonym. */
  native: string;
}

/** lib.rs UiSettings (data/settings.toml). */
export interface UiSettings {
  stt_provider: "groq" | "openrouter" | string;
  stt_model: string;
  llm_provider: "groq" | "openrouter" | string;
  llm_model: string;
  langs: string[];
  translate: boolean;
  translation_prompt: string;
  translation_mode: "fast" | "pro";
  cleanup: boolean;
  cleanup_ratio: number;
  trim_start_secs: number;
  trim_end_secs: number;
  target_chunk_secs: number;
  max_chunk_secs: number;
  max_flac_mb: number;
  noise_db: string;
  min_silence_secs: number;
  out_dir: string;
  profile: string;
}

/** core::JobId (newtype, serialised as a bare number). */
export type JobId = number;

/** core::JobEvent (serde tag = "type"). */
export type JobEvent =
  | { type: "stage_started"; job: JobId; stage: JobStage }
  | { type: "progress"; job: JobId; stage: JobStage; done: number; total: number }
  | { type: "cue_flagged"; job: JobId; cue_id: number; flag: unknown }
  | { type: "warning"; job: JobId; message: string }
  | { type: "cost"; job: JobId; tokens_in: number; tokens_out: number; usd: number | null }
  | { type: "stage_finished"; job: JobId; stage: JobStage }
  | { type: "job_failed"; job: JobId; error: string; retryable: boolean }
  | { type: "job_done"; job: JobId; outputs: string[] }
  | { type: "translation_removed"; job: JobId; langs: string[] };

export interface QueueSummary {
  done: number;
  failed: number;
  cancelled: boolean;
  error?: string;
}

export type LogKind = "info" | "ok" | "err" | "warn";

// Kept structured so the log panel never has to parse "iş 3: …" back apart.
export interface LogLine {
  /** Monotonic per session; doubles as the React key. */
  id: number;
  /** Epoch ms. */
  at: number;
  kind: LogKind;
  /** null for queue-level events. */
  jobId: JobId | null;
  stage: JobStage | null;
  /** Message only; job and stage are never baked in. */
  text: string;
}

export type LogDraft = Omit<LogLine, "id" | "at">;

// core::model

export interface Word {
  id: number;
  start_ms: number;
  end_ms: number;
  text: string;
}

export interface Track {
  lang: string;
  cues: Cue[];
  words?: Word[];
  flags?: { cue_id: number; flag: { type: string; cps?: number } }[];
}

export interface Cue {
  id: number;
  start_ms: number;
  end_ms: number;
  text: string;
  lines?: string[];
}
