import { useMemo } from "react";
import type { JobRow, UiSettings } from "../types";
import type { LogLine } from "../types";
import { IconAlert, IconPlay } from "./icons";
import ImportPanel from "./ImportPanel";
import StatusCard from "./StatusCard";
import JobList from "./JobList";
import LogPanel from "./LogPanel";

interface StudioProps {
  active: boolean;
  settings: UiSettings;
  jobs: JobRow[];
  queueRunning: boolean;
  logs: LogLine[];
  selectedJobId: number | null;
  onEnqueue: (inputs: string[]) => void;
  onStart: () => void;
  onCancel: () => void;
  onOpenJob: (jobId: number) => void;
  onRetryJob: (jobId: number) => void;
  onRemoveJob: (jobId: number) => void;
  onDeleteJob: (job: JobRow) => void;
  onPatchSettings: (patch: Partial<UiSettings>) => void;
  onOpenSettings: () => void;
  keyWarning: { title: string; text: string } | null;
  onFixKeys: () => void;
}

export default function Studio({
  active,
  settings,
  jobs,
  queueRunning,
  logs,
  selectedJobId,
  onEnqueue,
  onStart,
  onCancel,
  onOpenJob,
  onRetryJob,
  onRemoveJob,
  onDeleteJob,
  onPatchSettings,
  onOpenSettings,
  keyWarning,
  onFixKeys,
}: StudioProps) {
  // Queued jobs live in the import card, everything else in the library.
  const waitingJobs = useMemo(() => jobs.filter((j) => j.status === "queued"), [jobs]);
  const libraryJobs = useMemo(() => jobs.filter((j) => j.status !== "queued"), [jobs]);
  const hasQueued = waitingJobs.length > 0;

  return (
    <section id="view-studio" className={"view" + (active ? " active" : "")}>
      <header className="view-head">
        <div>
          <h1>Stüdyo</h1>
          <p className="view-sub">Videolarını yükle, altyazılarını çıkar ve çevir.</p>
        </div>
        <div className="head-actions">
          <button className="ghost" disabled={!queueRunning} onClick={onCancel}>
            İptal
          </button>
          <button className="accent" disabled={queueRunning || !hasQueued} onClick={onStart}>
            <IconPlay size={13} />
            Kuyruğu başlat
          </button>
        </div>
      </header>
      {keyWarning && (
        <div className="key-banner" role="alert">
          <IconAlert size={17} />
          <span>
            <strong>{keyWarning.title}</strong>
            {keyWarning.text}
          </span>
          <button className="primary btn-sm" onClick={onFixKeys}>
            Anahtarı ekle
          </button>
        </div>
      )}
      <div className="bento">
        <ImportPanel waitingJobs={waitingJobs} onEnqueue={onEnqueue} onRemove={onRemoveJob} />
        <StatusCard
          settings={settings}
          onOpenSettings={onOpenSettings}
          onToggleCleanup={(v) => onPatchSettings({ cleanup: v })}
          onToggleTranslate={(v) =>
            onPatchSettings(v ? { translate: true } : { translate: false, langs: [] })
          }
        />
        <JobList
          jobs={libraryJobs}
          selectedJobId={selectedJobId}
          onOpen={onOpenJob}
          onRetry={onRetryJob}
          onDelete={onDeleteJob}
        >
          <LogPanel lines={logs} />
        </JobList>
      </div>
    </section>
  );
}
