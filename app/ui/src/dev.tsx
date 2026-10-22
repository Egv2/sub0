// Browser-only design harness (npm run dev, then /dev.html): mock data, no Tauri.
// Params: ?empty, ?view=settings, ?theme=<id>.
import { createRoot } from "react-dom/client";
import { DEFAULT_STYLE } from "./subtitleStyle";
import Nav from "./components/Nav";
import Studio from "./components/Studio";
import SettingsView from "./components/Settings";
import type { LogLine } from "./types";
import type { JobRow, UiSettings } from "./types";
import { applyTheme, isThemeId, DEFAULT_THEME } from "./theme";
import "@fontsource-variable/inter";
import "./styles/global.css";

const settings: UiSettings = {
  stt_provider: "groq",
  stt_model: "whisper-large-v3",
  llm_provider: "openrouter",
  llm_model: "deepseek/deepseek-v4.1-flash",
  langs: ["tr", "en"],
  translate: true,
  translation_prompt: "",
  translation_mode: "pro",
  cleanup: true,
  cleanup_ratio: 0.2,
  trim_start_secs: 2,
  trim_end_secs: 5,
  target_chunk_secs: 600,
  max_chunk_secs: 900,
  max_flac_mb: 25,
  noise_db: "-38",
  min_silence_secs: 0.4,
  out_dir: "data/out",
  profile: "netflix",
};

const job = (over: Partial<JobRow>): JobRow => ({
  id: 0,
  batch_id: 1,
  source_kind: "file",
  source_ref: "",
  stage: "queued",
  status: "queued",
  error_class: null,
  error_msg: null,
  attempts: 1,
  out_dir: "",
  ...over,
});

const jobs: JobRow[] = [
  job({ id: 5, batch_id: 2, source_ref: "/Movies/podcast-bolum-12.mp4", stage: "done", status: "done" }),
  job({ id: 4, batch_id: 1, source_ref: "/Movies/test-video.mp4", stage: "done", status: "done" }),
  job({ id: 3, batch_id: 3, source_ref: "/Movies/konser-kaydi.mp4", stage: "transcribing", status: "running" }),
  job({ id: 2, batch_id: 3, source_ref: "https://www.youtube.com/watch?v=dQw4w9WgXcQ", stage: "queued", status: "queued" }),
  job({
    id: 1,
    batch_id: 2,
    source_ref: "https://vimeo.com/123456789",
    stage: "downloading",
    status: "failed",
    error_class: "network",
    error_msg: "bağlantı zaman aşımına uğradı",
    attempts: 2,
  }),
];

const logs: LogLine[] = [
  { id: 1, at: clock(0), kind: "info", jobId: null, stage: null, text: "2 girdi kuyruğa alındı" },
  { id: 2, at: clock(2), kind: "info", jobId: null, stage: null, text: "kuyruk başlatıldı" },
  { id: 3, at: clock(4), kind: "info", jobId: 3, stage: "transcribing", text: "konuşma metne dönüştürülüyor" },
  { id: 4, at: clock(9), kind: "warn", jobId: 3, stage: null, text: "4. bölüm düşük sesle kaydedilmiş, yeniden denenecek" },
  { id: 5, at: clock(12), kind: "err", jobId: 1, stage: null, text: "bağlantı zaman aşımına uğradı · tekrar denenecek" },
  { id: 6, at: clock(18), kind: "ok", jobId: 5, stage: null, text: "tamam · podcast-bolum-12.tr.srt, podcast-bolum-12.en.srt" },
  { id: 7, at: clock(21), kind: "ok", jobId: null, stage: null, text: "kuyruk bitti · 1 tamam, 1 başarısız" },
];

// Fixed date so the mock timestamps don't change between reloads.
function clock(sec: number): number {
  return new Date(2026, 0, 1, 19, 42, sec).getTime();
}

const params = new URLSearchParams(window.location.search);
const empty = params.has("empty");
const showSettings = params.get("view") === "settings";
const rawTheme = params.get("theme");
const themeParam = isThemeId(rawTheme) ? rawTheme : null;

// Note: this persists to localStorage too.
if (themeParam) applyTheme(themeParam);

createRoot(document.getElementById("root")!).render(
  <div className="shell">
    <Nav view={showSettings ? "settings" : "studio"} onNavigate={() => {}} queueState="running" />
    <main>
      {showSettings ? (
        <SettingsView
          active
          settings={settings}
          onSave={() => {}}
          theme={themeParam ?? DEFAULT_THEME}
          subtitleStyle={DEFAULT_STYLE}
        onEditSubtitleStyle={() => {}}
        onThemeChange={(t) => applyTheme(t)}
        />
      ) : (
        <Studio
          active
          settings={settings}
          jobs={empty ? [] : jobs}
          queueRunning={!empty}
          logs={empty ? [] : logs}
          selectedJobId={null}
          onEnqueue={() => {}}
          onStart={() => {}}
          onCancel={() => {}}
          onOpenJob={() => {}}
          onRetryJob={() => {}}
          onRemoveJob={() => {}}
          onDeleteJob={() => {}}
          onPatchSettings={() => {}}
          onOpenSettings={() => {}}
        />
      )}
    </main>
  </div>,
);
