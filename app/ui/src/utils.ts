import type { JobEvent, JobId, JobRow, LogDraft } from "./types";

/** Keyed by JobStage / JobStatus serde names. */
export const STAGE_TR: Record<string, string> = {
  queued: "sırada",
  downloading: "video indiriliyor",
  extracting: "ses dosyası hazırlanıyor",
  chunking: "uzun ses bölümleniyor",
  transcribing: "konuşma metne dönüştürülüyor",
  cleaning: "geçersiz cümleler temizleniyor",
  segmenting: "satırlar bölümleniyor",
  translating: "çevriliyor",
  validating: "kontrol ediliyor",
  exporting: "altyazı dosyası yazılıyor",
  done: "tamamlandı",
  failed: "başarısız oldu",
  cancelled: "durduruldu",
};

export const TERMINAL_STATUSES = ["done", "failed", "cancelled"];

// Go by `status`, not `stage`: a finished job can still have a stale stage.
export function statusTone(status: string): "ok" | "bad" | "run" | "idle" {
  if (status === "done") return "ok";
  if (status === "failed" || status === "cancelled") return "bad";
  if (status === "running") return "run";
  return "idle";
}

export function statusLabel(status: string, stage: string): string {
  const key = status === "running" ? stage : status;
  return STAGE_TR[key] ?? key;
}

/** 00:12.3 */
export function fmt(ms: number): string {
  const clamped = Math.max(0, Math.floor(ms));
  const s = Math.floor(clamped / 1000);
  const m = Math.floor(s / 60);
  return `${String(m).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}.${Math.floor((clamped % 1000) / 100)}`;
}

/** 19:42:07 */
export function fmtClock(at: number): string {
  const d = new Date(at);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`;
}

function baseName(p: string): string {
  return p.split("/").pop() ?? p;
}

// Returns null for high-frequency events (progress, cost, cue_flagged, stage_finished).
export function logFromEvent(ev: JobEvent): LogDraft | null {
  switch (ev.type) {
    case "stage_started":
      return {
        kind: "info",
        jobId: ev.job,
        stage: ev.stage,
        text: STAGE_TR[ev.stage] ?? ev.stage,
      };
    case "warning":
      return { kind: "warn", jobId: ev.job, stage: null, text: ev.message };
    case "job_done":
      return {
        kind: "ok",
        jobId: ev.job,
        stage: null,
        text: `tamam · ${ev.outputs.map(baseName).join(", ")}`,
      };
    case "job_failed":
      return {
        kind: "err",
        jobId: ev.job,
        stage: null,
        text: `${ev.error} · ${ev.retryable ? "tekrar denenecek" : "kalıcı hata"}`,
      };
    case "translation_removed":
      return {
        kind: "info",
        jobId: ev.job,
        stage: null,
        text: `çeviriler silindi · ${ev.langs.join(", ")}`,
      };
    case "progress":
    case "cue_flagged":
    case "cost":
    case "stage_finished":
      return null;
  }
}

export function logQueueDone(
  done: number,
  failed: number,
  cancelled: boolean,
  error?: string,
): LogDraft {
  if (error) return { kind: "err", jobId: null, stage: null, text: `kuyruk hatası · ${error}` };
  const parts = [`${done} tamam`];
  if (failed) parts.push(`${failed} başarısız`);
  if (cancelled) parts.push("durduruldu");
  return { kind: "ok", jobId: null, stage: null, text: `kuyruk bitti · ${parts.join(", ")}` };
}

// Translation results arrive as separate Tauri events, not core::JobEvent.
export function logTranslation(jobId: JobId, ok: boolean, detail: string): LogDraft {
  return {
    kind: ok ? "ok" : "err",
    jobId,
    stage: ok ? "translating" : null,
    text: ok ? `çeviri tamam · ${detail}` : `çeviri başarısız · ${detail}`,
  };
}

export function jobTitle(j: JobRow): string {
  if (j.source_kind === "file") {
    return j.source_ref.split("/").pop() ?? j.source_ref;
  }
  try {
    const u = new URL(j.source_ref);
    const last = u.pathname.split("/").filter(Boolean).pop() || u.hostname;
    return decodeURIComponent(last).slice(0, 70);
  } catch {
    return j.source_ref.slice(0, 70);
  }
}

/** 42:17, or 1:02:03 past an hour. */
export function fmtDuration(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(sec)}` : `${m}:${two(sec)}`;
}
