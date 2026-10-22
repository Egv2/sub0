// Typed wrappers around the Tauri commands in app/src-tauri/src/lib.rs.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, emit } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  ArtifactRow,
  JobEvent,
  JobRow,
  LanguageOption,
  QueueSummary,
  UiSettings,
} from "./types";
import type { ModelCatalog } from "./models";

async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return tauriInvoke<T>(cmd, args);
}

export const api = {
  loadSettings: () => invoke<UiSettings>("load_settings"),
  saveSettings: (settings: UiSettings) => invoke<void>("save_settings", { settings }),
  settingsPath: () => invoke<string>("settings_path"),
  keysStatus: () =>
    invoke<{
      GROQ_API_KEY: boolean;
      OPENROUTER_API_KEY: boolean;
      /** "file" = data/keys.json, "env" = environment, "" = not set. */
      sources?: Record<string, "file" | "env" | "">;
    }>("keys_status"),
  loadKeys: () => invoke<{ groq: string | null; openrouter: string | null }>("load_keys"),
  saveKeys: (patch: { groq?: string | null; openrouter?: string | null }) =>
    invoke<void>("save_keys", { patch }),
  keysPath: () => invoke<string>("keys_path"),
  languageOptions: () => invoke<LanguageOption[]>("language_options"),
  modelCatalog: (refresh = false) => invoke<ModelCatalog>("model_catalog", { refresh }),
  createBatch: (inputs: string[], settings: UiSettings) =>
    invoke<number>("create_batch", { inputs, settings }),
  startQueue: (settings: UiSettings | null) =>
    invoke<string>("start_queue", { settings, batchId: null, limit: null }),
  cancelQueue: () => invoke<void>("cancel_queue"),
  retryJob: (jobId: number) => invoke<void>("retry_job", { jobId }),
  /** Discards the output and runs the job again from scratch. */
  retranscribeJob: (jobId: number) => invoke<void>("retranscribe_job", { jobId }),
  removeJob: (jobId: number) => invoke<void>("remove_job", { jobId }),
  translateJob: (jobId: number, targets: string[]) =>
    invoke<void>("translate_job", { jobId, targets }),
  removeTranslation: (jobId: number, lang: string) =>
    invoke<void>("remove_translation", { jobId, lang }),
  exportFormats: () => invoke<string[]>("export_formats"),
  /** Returns the written paths; the backend picks the filenames. */
  exportTracks: (jobId: number, format: string, trackPaths: string[]) =>
    invoke<string[]>("export_tracks", { jobId, format, trackPaths }),
  /** The backend refuses unless `confirm` is true. */
  deleteProject: (jobId: number, confirm: boolean) =>
    invoke<void>("delete_project", { jobId, confirm }),
  listJobs: () => invoke<JobRow[]>("list_jobs", { batch: null }),
  listArtifacts: (jobId: number) => invoke<ArtifactRow[]>("list_artifacts", { jobId }),
  readArtifact: (path: string) => invoke<unknown>("read_artifact", { path }),
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),
  /** Opens the folder itself, unlike revealPath which selects it in its parent. */
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  openKeyPage: (provider: "groq" | "openrouter") => invoke<void>("open_key_page", { provider }),
};

export const events = {
  listenJobEvent: (h: (e: JobEvent) => void) => tauriListen<JobEvent>("job-event", (e) => h(e.payload)),
  listenQueueDone: (h: (s: QueueSummary) => void) =>
    tauriListen<QueueSummary>("queue-done", (e) => h(e.payload)),
  listenTranslationDone: (h: (p: { job_id: number; langs: string[] }) => void) =>
    tauriListen<{ job_id: number; langs: string[] }>("translation-done", (e) => h(e.payload)),
  listenTranslationFailed: (h: (p: { job_id: number; error: string }) => void) =>
    tauriListen<{ job_id: number; error: string }>("translation-failed", (e) => h(e.payload)),
  // Tauri's drag events give real file paths, unlike HTML5 drop.
  listenDrag: (h: (paths: string[] | null, phase: "enter" | "over" | "leave" | "drop") => void) => {
    const unsubs = [
      tauriListen<{ paths: string[] }>("tauri://drag-enter", (e) => h(e.payload?.paths ?? null, "enter")),
      tauriListen<unknown>("tauri://drag-over", () => h(null, "over")),
      tauriListen<unknown>("tauri://drag-leave", () => h(null, "leave")),
      tauriListen<{ paths: string[] }>("tauri://drag-drop", (e) => h(e.payload?.paths ?? [], "drop")),
    ];
    return () => { unsubs.forEach((p) => p.then((f) => f())); };
  },
};

// The main window stays hidden until this fires.
export const notifyFrontendReady = () => emit("frontend-ready");

export { convertFileSrc, open as pickFiles };
