import { useCallback, useEffect, useRef, useState } from "react";
import { api, events, notifyFrontendReady } from "./api";
import type {
  JobEvent,
  JobRow,
  LogDraft,
  LogLine,
  QueueSummary,
  UiSettings,
} from "./types";
import { applyTheme, readTheme, type ThemeId } from "./theme";
import { logFromEvent, logQueueDone, logTranslation } from "./utils";
import TitleBar from "./components/TitleBar";
import Nav, { type View } from "./components/Nav";
import Splash from "./components/Splash";
import Studio from "./components/Studio";
import LibraryView from "./components/LibraryView";
import SubtitleStyleModal from "./components/SubtitleStyleModal";
import { useSubtitleStyle } from "./useSubtitleStyle";
import Preview, { type TranslationNotice } from "./components/Preview";
import SettingsView from "./components/Settings";
import Onboarding from "./components/Onboarding";
import { PROVIDER_NAME, missingKeys, useKeyStatus } from "./useKeyStatus";

const ONBOARDED_KEY = "sub0.onboarded";

export default function App() {
  const [ready, setReady] = useState(false);
  const [view, setView] = useState<View>("studio");
  const [settings, setSettings] = useState<UiSettings | null>(null);
  const [theme, setTheme] = useState<ThemeId>(() => readTheme());
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [queueRunningState, setQueueRunningState] = useState(false);
  const [queueFailed, setQueueFailed] = useState(false);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [selectedJobId, setSelectedJobId] = useState<number | null>(null);
  const [translating, setTranslating] = useState<Record<number, string[]>>({});
  const [notice, setNotice] = useState<TranslationNotice | null>(null);
  const noticeN = useRef(0);
  const queueRunningRef = useRef(false);
  const logSeq = useRef(0);

  const setQueueRunning = useCallback((v: boolean) => {
    queueRunningRef.current = v;
    setQueueRunningState(v);
  }, []);

  const pushLog = useCallback((draft: LogDraft) => {
    logSeq.current += 1;
    const line: LogLine = { ...draft, id: logSeq.current, at: Date.now() };
    setLogs((ls) => [...ls.slice(-300), line]);
  }, []);

  const logInfo = useCallback(
    (text: string, jobId: number | null = null) =>
      pushLog({ kind: "info", jobId, stage: null, text }),
    [pushLog],
  );
  const logError = useCallback(
    (text: string, jobId: number | null = null) =>
      pushLog({ kind: "err", jobId, stage: null, text }),
    [pushLog],
  );

  const logEvent = useCallback(
    (ev: JobEvent) => {
      const draft = logFromEvent(ev);
      if (draft) pushLog(draft);
    },
    [pushLog],
  );

  const refreshJobs = useCallback(async () => {
    try {
      setJobs(await api.listJobs());
    } catch {
      /* if the store cannot open, wait quietly */
    }
  }, []);

  // Event subscriptions
  useEffect(() => {
    const subs = [
      events.listenJobEvent((ev) => {
        logEvent(ev);
        void refreshJobs();
      }),
      events.listenQueueDone((s: QueueSummary) => {
        pushLog(logQueueDone(s.done, s.failed, s.cancelled, s.error));
        setQueueRunning(false);
        setQueueFailed(Boolean(s.error));
        void refreshJobs();
      }),
      events.listenTranslationDone(({ job_id, langs }) => {
        setTranslating((t) => {
          const rest = { ...t };
          delete rest[job_id];
          return rest;
        });
        pushLog(logTranslation(job_id, true, langs.join(", ")));
        noticeN.current += 1;
        setNotice({ jobId: job_id, langs, n: noticeN.current });
        void refreshJobs();
      }),
      events.listenTranslationFailed(({ job_id, error }) => {
        setTranslating((t) => {
          const rest = { ...t };
          delete rest[job_id];
          return rest;
        });
        pushLog(logTranslation(job_id, false, error));
        noticeN.current += 1;
        setNotice({
          jobId: job_id,
          langs: [],
          failed: error,
          n: noticeN.current,
        });
        void refreshJobs();
      }),
    ];
    return () => {
      subs.forEach((p) => p.then((f) => f()));
    };
  }, [logEvent, pushLog, refreshJobs]);

  useEffect(() => {
    void (async () => {
      const [s, j] = await Promise.all([
        api.loadSettings(),
        api.listJobs().catch(() => [] as JobRow[]),
      ]);
      setSettings(s);
      setJobs(j);
      // The splash owns its minimum display time and reveals the window itself.
      setReady(true);
    })();
  }, []);

  const { status: keys, reload: reloadKeys } = useKeyStatus();
  const missing = settings
    ? missingKeys(keys, settings.stt_provider, settings.llm_provider)
    : { stt: null, llm: null };
  const [onboarding, setOnboarding] = useState<null | "intro" | "keys">(() =>
    localStorage.getItem(ONBOARDED_KEY) ? null : "intro",
  );
  const finishOnboarding = useCallback(() => {
    localStorage.setItem(ONBOARDED_KEY, "1");
    setOnboarding(null);
    void reloadKeys();
  }, [reloadKeys]);
  // Keys may change in Settings; re-check whenever the view changes.
  useEffect(() => {
    void reloadKeys();
  }, [view, reloadKeys]);

  const startQueue = useCallback(
    async (cfg: UiSettings | null) => {
      if (queueRunningRef.current) return;
      if (missing.stt) {
        logError(`${PROVIDER_NAME[missing.stt]} API anahtarı yok, kuyruk başlatılamadı`);
        setOnboarding("keys");
        return;
      }
      setQueueRunning(true);
      setQueueFailed(false);
      try {
        await api.startQueue(cfg);
        logInfo("kuyruk başlatıldı");
      } catch (e) {
        setQueueRunning(false);
        setQueueFailed(true);
        logError(String(e));
      }
    },
    [logInfo, logError, setQueueRunning, missing.stt],
  );

  const enqueue = useCallback(
    async (inputs: string[]) => {
      if (!inputs.length || !settings) return;
      try {
        await api.createBatch(inputs, settings);
        logInfo(`${inputs.length} girdi kuyruğa alındı`);
        await refreshJobs();
      } catch (e) {
        logError(String(e));
      }
    },
    [settings, logInfo, logError, refreshJobs],
  );

  const cancelQueue = useCallback(async () => {
    try {
      await api.cancelQueue();
    } catch (e) {
      logError(String(e));
    }
  }, [logInfo, logError]);

  const patchSettings = useCallback(
    async (patch: Partial<UiSettings>) => {
      if (!settings) return;
      const next = { ...settings, ...patch };
      setSettings(next);
      try {
        await api.saveSettings(next);
      } catch (e) {
        logError(`ayarlar kaydedilemedi · ${e}`);
      }
    },
    [settings, logError],
  );

  const subtitleStyle = useSubtitleStyle();
  const [styleOpen, setStyleOpen] = useState(false);

  const [libraryBusy, setLibraryBusy] = useState<number | null>(null);
  const [pendingDelete, setPendingDelete] = useState<JobRow | null>(null);
  const [noticeText, setNoticeText] = useState<string | null>(null);

  const deleteProject = useCallback(
    async (job: JobRow) => {
      setLibraryBusy(job.id);
      try {
        await api.deleteProject(job.id, true);
        setPendingDelete(null);
        if (selectedJobId === job.id) setSelectedJobId(null);
        await refreshJobs();
      } catch (e) {
        setNoticeText(typeof e === "string" ? e : "silinemedi");
      } finally {
        setLibraryBusy(null);
      }
    },
    [refreshJobs, selectedJobId],
  );

  const retranscribe = useCallback(
    async (job: JobRow) => {
      setLibraryBusy(job.id);
      try {
        await api.retranscribeJob(job.id);
        await refreshJobs();
      } catch (e) {
        setNoticeText(typeof e === "string" ? e : "yeniden işlenemedi");
      } finally {
        setLibraryBusy(null);
      }
    },
    [refreshJobs],
  );

  const retryJob = useCallback(
    async (jobId: number) => {
      try {
        await api.retryJob(jobId);
        logInfo("yeniden kuyruğa alındı", jobId);
      } catch (e) {
        if (/already running/.test(String(e)))
          logInfo("çalışan kuyruk tarafından işlenecek", jobId);
        else logError(String(e), jobId);
      }
      void refreshJobs();
    },
    [logInfo, logError, refreshJobs],
  );

  const removeJob = useCallback(
    async (jobId: number) => {
      try {
        await api.removeJob(jobId);
        logInfo("kuyruktan kaldırıldı", jobId);
      } catch (e) {
        logError(`kuyruktan kaldırılamadı · ${e}`, jobId);
      }
      void refreshJobs();
    },
    [logInfo, logError, refreshJobs],
  );

  const translate = useCallback(
    async (jobId: number, targets: string[]) => {
      try {
        await api.translateJob(jobId, targets);
        setTranslating((t) => ({ ...t, [jobId]: targets }));
        logInfo(`çeviri başladı · ${targets.join(", ")}`, jobId);
      } catch (e) {
        logError(`çeviri başlatılamadı · ${e}`, jobId);
      }
    },
    [logInfo, logError],
  );

  const [previewOrigin, setPreviewOrigin] = useState<View>("studio");
  const openPreview = useCallback((jobId: number, origin: View = "studio") => {
    setPreviewOrigin(origin);
    setSelectedJobId(jobId);
    setView("preview");
  }, []);
  const leavePreview = useCallback(() => {
    setSelectedJobId(null);
    setView(previewOrigin);
  }, [previewOrigin]);
  const changeTheme = useCallback((t: ThemeId) => {
    setTheme(t);
    applyTheme(t);
  }, []);

  // Same key in both returns so the splash (and its video) is never remounted.
  const splash = <Splash key="splash" ready={ready} onVisible={() => void notifyFrontendReady()} />;
  if (!settings) return <>{splash}</>;

  const selectedJob = jobs.find((j) => j.id === selectedJobId) ?? null;
  const queueState = queueRunningState
    ? "running"
    : queueFailed
      ? "failed"
      : "idle";

  return (
    <>
      <TitleBar />
      <div className="shell">
        <Nav view={view} onNavigate={setView} queueState={queueState} />
        <main>
          <Studio
            active={view === "studio"}
            settings={settings}
            jobs={jobs}
            queueRunning={queueRunningState}
            logs={logs}
            selectedJobId={selectedJobId}
            onEnqueue={(inputs) => void enqueue(inputs)}
            onStart={() => void startQueue(null)}
            onCancel={() => void cancelQueue()}
            onOpenJob={(id) => openPreview(id, "studio")}
            onRetryJob={(id) => void retryJob(id)}
            onRemoveJob={(id) => void removeJob(id)}
            onDeleteJob={(job) => void deleteProject(job)}
            onPatchSettings={(patch) => void patchSettings(patch)}
            onOpenSettings={() => setView("settings")}
            keyWarning={
              missing.stt
                ? {
                    title: `${PROVIDER_NAME[missing.stt]} API anahtarı eksik.`,
                    text: "Bu anahtar olmadan altyazı üretilemez.",
                  }
                : missing.llm && settings.translate
                  ? {
                      title: `${PROVIDER_NAME[missing.llm]} API anahtarı eksik.`,
                      text: "Çeviri ve akıllı temizlik çalışmaz.",
                    }
                  : null
            }
            onFixKeys={() => setOnboarding("keys")}
          />
          {/* Preview landing: a selected job opens directly, otherwise the
            library screen offers the finished work. */}
          {!selectedJob && view === "preview" && (
            <LibraryView
              jobs={jobs}
              busy={libraryBusy}
              onOpen={(id) => openPreview(id, "preview")}
              onDelete={(j) => setPendingDelete(j)}
              onRetranscribe={(j) => void retranscribe(j)}
            />
          )}
          {selectedJob && (
            <Preview
              active={view === "preview"}
              job={selectedJob}
              jobs={jobs}
              settings={settings}
              translating={translating[selectedJob.id] ?? []}
              notice={notice}
              onBack={leavePreview}
              onSelectJob={(id) => openPreview(id, previewOrigin)}
              onTranslate={(id, targets) => void translate(id, targets)}
              onReveal={(path) => void api.revealPath(path)}
              onOpenSettings={() => setView("settings")}
              onDeleted={() => {
                void refreshJobs().then(() => setView("studio"));
              }}
            />
          )}
          {styleOpen && (
            <SubtitleStyleModal
              style={subtitleStyle.style}
              onApply={(next) => {
                subtitleStyle.update(next);
                setStyleOpen(false);
              }}
              onClose={() => setStyleOpen(false)}
            />
          )}
          {/* Silme geri alınamaz: onay istenir. Kütüphane ekranından gelen
            silme isteği burada karşılanır, aynı akış her yerden aynı. */}
          {pendingDelete && (
            <div
              id="delete-backdrop"
              onClick={() => !libraryBusy && setPendingDelete(null)}
            >
              <div
                className="confirm-card"
                role="alertdialog"
                aria-modal="true"
                onClick={(e) => e.stopPropagation()}
              >
                <h2>Proje silinsin mi?</h2>
                <p>
                  <b>{pendingDelete.source_ref}</b> için üretilen tüm altyazılar
                  ve dosyalar kalıcı olarak silinir. Bu işlem geri alınamaz.
                </p>
                <div className="confirm-actions">
                  <button
                    className="ghost"
                    onClick={() => setPendingDelete(null)}
                  >
                    Vazgeç
                  </button>
                  <button
                    className="ghost danger"
                    disabled={libraryBusy != null}
                    onClick={() => void deleteProject(pendingDelete)}
                  >
                    {libraryBusy != null ? "Siliniyor…" : "Evet, sil"}
                  </button>
                </div>
              </div>
            </div>
          )}
          {noticeText && (
            <div
              className="toast"
              role="status"
              onClick={() => setNoticeText(null)}
            >
              {noticeText}
            </div>
          )}
          <SettingsView
            subtitleStyle={subtitleStyle.style}
            onEditSubtitleStyle={() => setStyleOpen(true)}
            active={view === "settings"}
            settings={settings}
            onSave={(s) => setSettings(s)}
            theme={theme}
            onThemeChange={changeTheme}
            onReplayTutorial={() => setOnboarding("intro")}
          />
        </main>
      </div>
      {onboarding && ready && (
        <Onboarding
          key="onboarding"
          keys={keys}
          sttProvider={settings.stt_provider}
          llmProvider={settings.llm_provider}
          startAtKeys={onboarding === "keys"}
          onKeysChanged={() => void reloadKeys()}
          onDone={finishOnboarding}
        />
      )}
      {splash}
    </>
  );
}
