import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import type { JobRow } from "../types";
import { jobTitle, statusLabel } from "../utils";
import { Card, CardHead } from "./ui";
import { IconArchive, IconArrowRight, IconTrash } from "./icons";
import loadingGif from "../assets/loading.gif";

interface JobListProps {
  jobs: JobRow[];
  selectedJobId: number | null;
  onOpen: (jobId: number) => void;
  onRetry: (jobId: number) => void;
  /** Deletes the job and its folder. */
  onDelete: (job: JobRow) => void;
  /** Rendered below the list (the event log). */
  children?: ReactNode;
}

function StatusIcon({ job }: { job: JobRow }) {
  if (job.status === "done") {
    return (
      <svg viewBox="0 0 24 24">
        <circle className="ok-ring" cx="12" cy="12" r="9" />
        <path className="ok-mark" d="m8.3 12.3 2.5 2.5 5-5.4" />
      </svg>
    );
  }
  if (job.status === "failed") {
    return (
      <svg viewBox="0 0 24 24">
        <circle className="failed" cx="12" cy="12" r="9" />
        <path className="failed" d="M12 7.5v5.5m0 3.4v.01" />
      </svg>
    );
  }
  if (job.status === "running") {
    return <img className="job-loading" src={loadingGif} alt="" />;
  }
  return (
    <svg viewBox="0 0 24 24">
      <circle className="queued" cx="12" cy="12" r="9" />
      <path className="queued" d="M12 7.5V12l3 2" />
    </svg>
  );
}

export default function JobList({ jobs, selectedJobId, onOpen, onRetry, onDelete, children }: JobListProps) {
  const [langTags, setLangTags] = useState<Record<number, string>>({});

  // One artifact query per finished job.
  useEffect(() => {
    let stale = false;
    (async () => {
      for (const j of jobs) {
        if (j.status !== "done" || langTags[j.id] !== undefined) continue;
        const arts = await api.listArtifacts(j.id).catch(() => []);
        if (stale) return;
        const langs = [
          ...new Set(
            arts
              .filter((a) => a.kind === "srt")
              .map((a) => (a.path.match(/\/([a-z]{2,3}(?:-[A-Za-z]{2,4})?)?\.srt$/) ?? [])[1])
              .filter((x): x is string => Boolean(x)),
          ),
        ];
        setLangTags((m) => ({ ...m, [j.id]: langs.length ? `SRT: ${langs.join(", ")}` : "tamamlandı" }));
      }
    })();
    return () => {
      stale = true;
    };
  }, [jobs]);

  // Two-click delete; the armed state times out so a stray click doesn't linger.
  const [armedDelete, setArmedDelete] = useState<number | null>(null);
  useEffect(() => {
    if (armedDelete === null) return;
    const t = setTimeout(() => setArmedDelete(null), 4000);
    return () => clearTimeout(t);
  }, [armedDelete]);
  const askDelete = (j: JobRow) => {
    if (armedDelete !== j.id) {
      setArmedDelete(j.id);
      return;
    }
    setArmedDelete(null);
    onDelete(j);
  };

  // The attention cue fires when running rows actually show up (after the
  // refreshJobs round-trip), not when Start is pressed. Diff against the last
  // poll so only newly running ids animate.
  const runningIds = useMemo(
    () => new Set(jobs.filter((j) => j.status === "running").map((j) => j.id)),
    [jobs],
  );
  const prevRunningRef = useRef<Set<number>>(new Set());
  const [freshIds, setFreshIds] = useState<Set<number>>(() => new Set());
  const [pulse, setPulse] = useState(0);

  useEffect(() => {
    const prev = prevRunningRef.current;
    const fresh = [...runningIds].filter((id) => !prev.has(id));
    prevRunningRef.current = runningIds;
    if (fresh.length === 0) return;
    setFreshIds(new Set(fresh));
    setPulse((p) => p + 1);
  }, [runningIds]);

  // Rows land by ~620ms and the glow by ~1100ms; clear after so the cue can replay.
  useEffect(() => {
    if (pulse === 0) return;
    const t = setTimeout(() => setFreshIds(new Set()), 1400);
    return () => clearTimeout(t);
  }, [pulse]);

  const counts = {
    done: jobs.filter((j) => j.status === "done").length,
    running: jobs.filter((j) => j.status === "running").length,
  };

  const rows = jobs.slice().sort((a, b) => b.id - a.id);
  // Stagger in visual order (newest first), capped at 8.
  const delayOf = new Map<number, number>();
  if (freshIds.size > 0) {
    let n = 0;
    for (const j of rows) {
      if (freshIds.has(j.id)) delayOf.set(j.id, Math.min(n++, 8));
    }
  }

  return (
    <Card className={"bento-library grow" + (freshIds.size > 0 ? " attn" : "")}>
      <CardHead
        icon={<IconArchive size={16} />}
        title="Kütüphane"
        right={
          jobs.length > 0 ? (
            <span className="lib-summary">
              {counts.done} tamam · {counts.running} işleniyor
            </span>
          ) : undefined
        }
      />
      <div className="lib">
        {rows
          .map((j) => {
            const clickable = j.status === "done";
            const arrive = delayOf.get(j.id);
            const stage = statusLabel(j.status, j.stage);
            let meta: React.ReactNode = `batch ${j.batch_id}`;
            if (j.status === "running") {
              meta = (
                <>
                  batch {j.batch_id}
                  <span className="sep">·</span>
                  <span className="stage-running">{stage}…</span>
                </>
              );
            } else if (j.status === "queued") {
              meta = (
                <>
                  batch {j.batch_id}
                  <span className="sep">·</span>sırada
                </>
              );
            } else if (j.status === "failed") {
              meta = (
                <>
                  batch {j.batch_id}
                  <span className="sep">·</span>
                  <span className="badge failed">{stage}</span>
                  {j.error_msg && (
                    <span className="hint" title={j.error_msg}>
                      {" "}
                      {j.error_msg.slice(0, 60)}
                    </span>
                  )}
                </>
              );
            } else if (j.status === "done") {
              meta = (
                <>
                  batch {j.batch_id}
                  <span className="sep">·</span>
                  <span className="lang-tag">{langTags[j.id] ?? "tamamlandı"}</span>
                </>
              );
            }
            return (
              <div
                key={j.id}
                className={
                  "lib-row" +
                  (clickable ? " clickable" : "") +
                  (j.id === selectedJobId ? " selected" : "") +
                  (arrive !== undefined ? " arriving" : "")
                }
                style={arrive !== undefined ? ({ "--d": arrive } as React.CSSProperties) : undefined}
                onClick={clickable ? () => onOpen(j.id) : undefined}
              >
                <div className="st-icon">
                  <StatusIcon job={j} />
                </div>
                <div className="lib-main">
                  <div className="lib-title" title={j.source_ref}>
                    {jobTitle(j)}
                  </div>
                  <div className="lib-meta">{meta}</div>
                </div>
                <div className="lib-side">
                  {clickable && (
                    <span className="open-hint">
                      Önizle <IconArrowRight size={13} />
                    </span>
                  )}
                  {j.status === "failed" && (
                    <button
                      className="ghost btn-sm"
                      onClick={(e) => {
                        e.stopPropagation();
                        onRetry(j.id);
                      }}
                    >
                      Yeniden dene
                    </button>
                  )}
                  {/* Finished jobs are deleted from the library view instead. */}
                  {j.status !== "done" && (
                    <button
                      className={"ghost btn-sm lib-cancel" + (armedDelete === j.id ? " armed" : "")}
                      title={
                        armedDelete === j.id
                          ? "Silmek için tekrar tıkla"
                          : "İşi iptal et ve klasörünü sil"
                      }
                      aria-label={`${jobTitle(j)} işini iptal et`}
                      onClick={(e) => {
                        e.stopPropagation();
                        askDelete(j);
                      }}
                    >
                      {armedDelete === j.id ? (
                        "Emin misin?"
                      ) : (
                        <>
                          <IconTrash size={12} />
                          <span className="sr-only">İptal</span>
                        </>
                      )}
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        {jobs.length === 0 && <div className="empty">Henüz iş yok — video aktararak başla.</div>}
      </div>
      {children}
    </Card>
  );
}
