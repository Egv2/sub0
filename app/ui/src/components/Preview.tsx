// Karaoke highlighting and cue tracking touch the DOM directly from a rAF loop
// to avoid re-rendering at 60 fps.
import { useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { api, convertFileSrc } from "../api";
import type { ArtifactRow, Cue, JobRow, Track, UiSettings, Word } from "../types";
import { fmt, fmtDuration, jobTitle, statusLabel, statusTone } from "../utils";
import { langName, useLanguages } from "../languages";
import TranslateBar from "./TranslateBar";
import ExportModal from "./ExportModal";
import {
  IconAlert,
  IconArrowLeft,
  IconCaptions,
  IconClock,
  IconDownload,
  IconExpand,
  IconFileText,
  IconFilter,
  IconGlobe,
  IconGear,
  IconPlay,
  IconSearch,
  IconTrash,
} from "./icons";

const ACCENTS = ["#6ea8ff", "#4cd47c", "#ffcc4d", "#ff7ab6"];

export interface UiTrack {
  key: string;
  label: string;
  kind: "track" | "translation";
  lang: string;
  accent: string;
  cues: Cue[];
  words: Word[];
  cueWords: number[][];
  flags: { cue_id: number; flag: { type: string; cps?: number } }[];
  path: string;
}

export interface TranslationNotice {
  jobId: number;
  langs: string[];
  failed?: string;
  n: number;
}

interface PreviewProps {
  active: boolean;
  job: JobRow;
  jobs: JobRow[];
  settings: UiSettings;
  translating: string[];
  notice: TranslationNotice | null;
  onBack: () => void;
  onSelectJob: (jobId: number) => void;
  onTranslate: (jobId: number, targets: string[]) => void;
  onReveal: (path: string) => void;
  onOpenSettings: () => void;
  onDeleted: (jobId: number) => void;
}

function cueIndexAt(cues: Cue[], t: number): number {
  let lo = 0;
  let hi = cues.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (cues[mid].start_ms <= t) {
      ans = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return ans;
}

// Keep in sync with `Profile` in crates/core.
const NETFLIX = { maxChars: 42, lines: 2, maxCps: 17, minMs: 833, gapMs: 83 };

function buildTracks(
  artifacts: ArtifactRow[],
  words: Word[] | null,
  loaded: Map<string, Track>,
): UiTrack[] {
  // Use the artifact's `lang`, not the filename. Empty `lang` = source transcript.
  const defs: { track: Track; label: string; kind: "track" | "translation"; path: string }[] = [];
  const tracks = artifacts.filter((a) => a.kind === "track");
  for (const a of tracks) {
    const t = loaded.get(a.path);
    if (!t) continue;
    const isSource = !a.lang;
    defs.push({
      track: t,
      label: isSource ? "kaynak" : t.lang || a.lang,
      kind: isSource ? "track" : "translation",
      path: a.path,
    });
  }
  return defs.map((d, i) => {
    const cues = d.track.cues.slice().sort((a, b) => a.start_ms - b.start_ms);
    const w = d.kind === "track" && words ? words.slice().sort((a, b) => a.start_ms - b.start_ms) : [];
    const cueWords: number[][] = cues.map(() => []);
    let ci = 0;
    for (let wi = 0; wi < w.length; wi++) {
      while (ci + 1 < cues.length && cues[ci + 1].start_ms <= w[wi].start_ms) ci++;
      cueWords[ci].push(wi);
    }
    return {
      key: "t" + i,
      label: d.label,
      kind: d.kind,
      lang: d.track.lang || d.label,
      accent: ACCENTS[i % ACCENTS.length],
      cues,
      words: w,
      cueWords,
      path: d.path,
      flags: d.track.flags ?? [],
    };
  });
}

export default function Preview({
  active,
  job,
  jobs,
  settings,
  translating,
  notice,
  onBack,
  onSelectJob,
  onTranslate,
  onReveal,
  onOpenSettings,
  onDeleted,
}: PreviewProps) {
  useLanguages();
  const videoRef = useRef<HTMLVideoElement>(null);
  const cueListRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [tracks, setTracks] = useState<UiTrack[]>([]);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [pending, setPending] = useState<string[]>([]);
  const [removing, setRemoving] = useState<string[]>([]);
  const [formats, setFormats] = useState<string[]>(["srt"]);
  const [exportOpen, setExportOpen] = useState(false);
  // "all" or a single language code. Deliberately not persisted in settings.
  const [view, setView] = useState<string>("all");
  const [exporting, setExporting] = useState(false);
  const [deleteAsk, setDeleteAsk] = useState(false);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [reloadTick, setReloadTick] = useState(0);
  const [actionError, setActionError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [query, setQuery] = useState("");
  const [onlyFlagged, setOnlyFlagged] = useState(false);
  const [durationMs, setDurationMs] = useState<number | null>(null);
  const [heightP, setHeightP] = useState<number | null>(null);
  const [playing, setPlaying] = useState(false);
  const deferredQuery = useDeferredValue(query);

  const reloadN = notice && notice.jobId === job.id ? notice.n : 0;

  useEffect(() => {
    setQuery("");
    setOnlyFlagged(false);
    setDurationMs(null);
    setHeightP(null);
    setPlaying(false);
  }, [job.id]);

  useEffect(() => {
    let stale = false;
    setTracks([]);
    setActiveKey(null);
    (async () => {
      const artifacts = await api.listArtifacts(job.id).catch(() => []);
      if (stale) return;
      // track/srt have one record per language, so only map the singular kinds.
      const byKind = new Map(
        artifacts.filter((a) => a.kind === "source" || a.kind === "words").map((a) => [a.kind, a.path]),
      );
      const loaded = new Map<string, Track>();
      const wordsPath = byKind.get("words");
      const words = wordsPath
        ? ((await api.readArtifact(wordsPath).catch(() => null)) as { words: Word[] } | null)?.words ?? null
        : null;
      for (const a of artifacts.filter((a) => a.kind === "track")) {
        const t = (await api.readArtifact(a.path).catch(() => null)) as Track | null;
        if (t) loaded.set(a.path, t);
        if (stale) return;
      }
      const built = buildTracks(artifacts, words, loaded);
      if (stale) return;
      setTracks(built);
      setPending(settings.langs.filter((l) => !built.some((t) => t.lang === l)));
      if (built.length) setActiveKey(built[0].key);

      const videoPath = byKind.get("source") ?? (job.source_kind === "file" ? job.source_ref : "");
      if (videoPath && videoRef.current) {
        videoRef.current.src = convertFileSrc(videoPath);
      }
    })();
    return () => {
      stale = true;
    };
  }, [job.id, job.source_kind, job.source_ref, reloadN, reloadTick, settings.langs]);

  useEffect(() => {
    let raf = 0;
    let lastCur = -1;
    const tick = () => {
      const v = videoRef.current;
      if (v) {
        const t = v.currentTime * 1000;
        // Only show the active track; otherwise languages stack on top of each other.
        const shown = new Set<string>();
        for (const p of tracks) {
          const el = document.getElementById("cap-" + p.key);
          if (!el) continue;
          const i = cueIndexAt(p.cues, t);
          const show = i >= 0 && t <= p.cues[i].end_ms;
          if (!show || p.key !== activeKey) {
            el.classList.remove("on");
            continue;
          }
          shown.add(p.key);
          el.classList.add("on");
          const cue = p.cues[i];
          const wis = p.cueWords[i] ?? [];
          const tokens = cue.text.split(/\s+/).filter(Boolean);
          if (wis.length && tokens.length === wis.length) {
            let k = 0;
            const word = (tok: string) => {
              const w = p.words[wis[k++]];
              const cur = w && t >= w.start_ms && t < w.end_ms;
              return `<w class="${cur ? "cur" : ""}">${tok}</w>`;
            };
            el.innerHTML = cue.lines
              ? cue.lines
                  .map((l) =>
                    l
                      .split(" ")
                      .filter(Boolean)
                      .map((tok) => word(tok.replace(/&/g, "&amp;").replace(/</g, "&lt;")))
                      .join(" "),
                  )
                  .join("<br>")
              : tokens
                  .map((tok) => word(tok.replace(/&/g, "&amp;").replace(/</g, "&lt;")))
                  .join(" ");
          } else {
            el.textContent = cue.text;
          }
        }
        const active = tracks.find((tr) => tr.key === activeKey);
        const list = cueListRef.current;
        if (active && list) {
          const i = cueIndexAt(active.cues, t);
          if (i !== lastCur) {
            list.querySelectorAll(".cue.cur").forEach((el) => el.classList.remove("cur"));
            if (i >= 0) {
              const el = list.querySelector(`[data-cue-index="${i}"]`);
              if (el) {
                el.classList.add("cur");
                el.scrollIntoView({ block: "nearest" });
              }
            }
            lastCur = i;
          }
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // Filters are deps so a row that reappears gets its highlight back.
  }, [tracks, activeKey, deferredQuery, onlyFlagged]);

  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setExpanded(false);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [expanded]);

  const splitRef = useRef<HTMLDivElement>(null);
  const videoPaneRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const split = splitRef.current;
    const pane = videoPaneRef.current;
    const gutter = split?.querySelector("#gutter");
    if (!split || !pane || !gutter) return;
    let dragging = false;
    const down = (e: Event) => {
      e.preventDefault();
      dragging = true;
      gutter.classList.add("dragging");
      document.body.style.cursor = "col-resize";
    };
    const move = (e: MouseEvent) => {
      if (!dragging) return;
      const r = split.getBoundingClientRect();
      const pct = Math.min(75, Math.max(30, ((e.clientX - r.left) / r.width) * 100));
      pane.style.width = pct + "%";
    };
    const up = () => {
      dragging = false;
      gutter.classList.remove("dragging");
      document.body.style.cursor = "";
    };
    gutter.addEventListener("mousedown", down);
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    return () => {
      gutter.removeEventListener("mousedown", down);
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
  }, [active]);

  const activeTrack = tracks.find((t) => t.key === activeKey) ?? null;
  const cueCount = activeTrack?.cues.length ?? 0;

  // Rows keep their original index so the rAF loop can find them by data-cue-index.
  const visible = useMemo(() => {
    if (!activeTrack) return [] as { cue: Cue; index: number; flags: { cue_id: number; flag: { type: string; cps?: number } }[] }[];
    const q = deferredQuery.trim().toLocaleLowerCase("tr");
    return activeTrack.cues
      .map((cue, index) => ({ cue, index, flags: activeTrack.flags.filter((f) => f.cue_id === cue.id) }))
      .filter(
        ({ cue, flags }) =>
          (!onlyFlagged || flags.length > 0) && (!q || cue.text.toLocaleLowerCase("tr").includes(q)),
      );
  }, [activeTrack, deferredQuery, onlyFlagged]);

  const seek = useCallback((ms: number) => {
    const v = videoRef.current;
    if (v) {
      v.currentTime = ms / 1000;
      void v.play();
    }
  }, []);

  const togglePlay = useCallback(() => {
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) void v.play();
    else v.pause();
  }, []);

  // Leaving the preview (Library, Studio, Settings) must not leave audio playing in the background.
  useEffect(() => {
    if (!active) videoRef.current?.pause();
  }, [active]);

  const skip = useCallback((sec: number) => {
    const v = videoRef.current;
    if (!v) return;
    const end = Number.isFinite(v.duration) ? v.duration : Infinity;
    v.currentTime = Math.min(end, Math.max(0, v.currentTime + sec));
  }, []);

  // Space/K play-pause, ←/→ seek 5s (Shift: 1s), J/L seek 10s, ↑/↓ step through cues,
  // M mute, F expand, Home/End jump, / focuses search, Esc clears it.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing =
        !!target &&
        (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT" || target.isContentEditable);
      if (e.key === "Escape") {
        if (!expanded && document.activeElement === searchRef.current && query) setQuery("");
        return;
      }
      if (typing || exportOpen || deleteAsk) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      // The native video controls already handle keys while focused; don't double-fire.
      if (target?.tagName === "VIDEO") return;
      const v = videoRef.current;
      const onControl = target?.tagName === "BUTTON" || target?.classList.contains("cue");
      switch (e.key) {
        case " ":
          // Space on a focused button/cue row activates it instead.
          if (onControl) return;
          e.preventDefault();
          togglePlay();
          return;
        case "k":
        case "K":
          e.preventDefault();
          togglePlay();
          return;
        case "ArrowLeft":
        case "ArrowRight":
          e.preventDefault();
          skip((e.key === "ArrowLeft" ? -1 : 1) * (e.shiftKey ? 1 : 5));
          return;
        case "j":
        case "J":
        case "l":
        case "L":
          e.preventDefault();
          skip(e.key.toLowerCase() === "j" ? -10 : 10);
          return;
        case "m":
        case "M":
          if (v) v.muted = !v.muted;
          return;
        case "f":
        case "F":
          setExpanded((x) => !x);
          return;
        case "Home":
          if (v) v.currentTime = 0;
          e.preventDefault();
          return;
        case "End":
          if (v && Number.isFinite(v.duration)) v.currentTime = v.duration;
          e.preventDefault();
          return;
      }
      if (e.key === "/") {
        e.preventDefault();
        searchRef.current?.focus();
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      const track = tracks.find((t) => t.key === activeKey);
      if (!track?.cues.length) return;
      e.preventDefault();
      const now = v ? v.currentTime * 1000 : 0;
      const cur = cueIndexAt(track.cues, now);
      const next = e.key === "ArrowDown" ? Math.min(track.cues.length - 1, cur + 1) : Math.max(0, cur - 1);
      if (track.cues[next]) seek(track.cues[next].start_ms);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, activeKey, deleteAsk, expanded, exportOpen, query, seek, skip, togglePlay, tracks]);

  const onMeta = useCallback(() => {
    const v = videoRef.current;
    if (!v) return;
    setDurationMs(Number.isFinite(v.duration) && v.duration > 0 ? Math.round(v.duration * 1000) : null);
    setHeightP(v.videoHeight > 0 ? v.videoHeight : null);
  }, []);

  useEffect(() => {
    let alive = true;
    api
      .exportFormats()
      .then((f) => alive && f.length && setFormats(f))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  // File naming lives in the backend; the UI only sends paths and a format.
  const runExport = useCallback(
    async (paths: string[], format: string) => {
      if (!paths.length) return;
      setExporting(true);
      setActionError(null);
      try {
        const files = await api.exportTracks(job.id, format, paths);
        setExportOpen(false);
        // The files are already written, so a failure to open the folder isn't an export error.
        try {
          if (files.length) await api.openFolder(job.out_dir);
        } catch {
          onReveal(job.out_dir);
        }
      } catch (e) {
        setActionError(typeof e === "string" ? e : "dışa aktarılamadı");
      } finally {
        setExporting(false);
      }
    },
    [job.id, job.out_dir, onReveal],
  );

  const confirmDelete = useCallback(async () => {
    setDeleteBusy(true);
    try {
      await api.deleteProject(job.id, true);
      setDeleteAsk(false);
      onDeleted(job.id);
    } catch (e) {
      setActionError(typeof e === "string" ? e : "silinemedi");
    } finally {
      setDeleteBusy(false);
    }
  }, [job.id, onDeleted]);

  const togglePending = useCallback(
    (code: string) => {
      setPending((p) => (p.includes(code) ? p.filter((c) => c !== code) : [...p, code]));
    },
    [],
  );

  const removeTranslation = useCallback(
    async (lang: string) => {
      setRemoving((r) => (r.includes(lang) ? r : [...r, lang]));
      try {
        await api.removeTranslation(job.id, lang);
        setReloadTick((n) => n + 1);
      } catch (e) {
        setActionError(typeof e === "string" ? e : "çeviri silinemedi");
      } finally {
        setRemoving((r) => r.filter((c) => c !== lang));
      }
    },
    [job.id],
  );

  const startTranslate = useCallback(() => {
    if (!pending.length) return;
    onTranslate(job.id, pending);
    setPending([]);
  }, [job.id, onTranslate, pending]);

  const viewTracks = useMemo(
    () => (view === "all" ? tracks : tracks.filter((t) => t.key === view)),
    [tracks, view],
  );

  // Fall back to a visible track if the view hides the active one.
  useEffect(() => {
    if (!viewTracks.some((t) => t.key === activeKey)) {
      setActiveKey(viewTracks[0]?.key ?? null);
    }
  }, [viewTracks, activeKey]);

  const flagged = activeTrack
    ? activeTrack.cues.filter((c) => activeTrack.flags.some((f) => f.cue_id === c.id)).length
    : 0;

  const status = useMemo(() => {
    if (translating.length) return `çeviri sürüyor — ${translating.join(", ")}`;
    if (notice && notice.jobId === job.id) {
      return notice.failed ? `çeviri başarısız: ${notice.failed.slice(0, 60)}` : "çeviri tamamlandı";
    }
    return "";
  }, [translating, notice, job.id]);

  const doneJobs = jobs.filter((j) => j.status === "done");
  const tone = statusTone(job.status);

  return (
    <section id="view-preview" className={"view" + (active ? " active" : "")}>
      <header className="pv-head">
        <button className="icon-btn pv-back" title="Stüdyoya dön" onClick={onBack}>
          <IconArrowLeft size={16} />
        </button>
        <div className="pv-titles">
          <div className="pv-title-row">
            <h1>{jobTitle(job)}</h1>
            <span className={"pv-status " + tone}>
              <i className="pv-status-dot" />
              {statusLabel(job.status, job.stage)}
            </span>
          </div>
          <div className="pv-meta">
            {durationMs != null && (
              <span className="pv-meta-item">
                <IconClock size={12} /> {fmtDuration(durationMs)}
              </span>
            )}
            {heightP != null && (
              <span className="pv-meta-item">
                <IconCaptions size={12} /> {heightP}p
              </span>
            )}
            {activeTrack && (
              <span className="pv-meta-item">
                <IconGlobe size={12} /> {langName(activeTrack.lang)}
              </span>
            )}
            <span className="pv-meta-item">
              <IconFileText size={12} /> {cueCount} satır
            </span>
          </div>
        </div>
        <div className="pv-actions">
          <button className="primary" onClick={() => setExportOpen(true)}>
            <IconDownload size={14} /> Dışa aktar
          </button>
          <button className="ghost icon" title="Tam ekran (F)" onClick={() => setExpanded(true)}>
            <IconExpand size={15} />
          </button>
          <label className="sr-only" htmlFor="pv-job">
            Tamamlanmış iş seç
          </label>
          <select
            id="pv-job"
            className="ctl text slim"
            value={job.id}
            onChange={(e) => onSelectJob(+e.target.value)}
          >
            {doneJobs.map((j) => (
              <option key={j.id} value={j.id}>
                {jobTitle(j)}
              </option>
            ))}
          </select>
          <button className="icon-btn" title="Bu işi sil" onClick={() => setDeleteAsk(true)}>
            <IconTrash size={15} />
          </button>
          <button
            className="icon-btn"
            title="Uygulama ayarları"
            aria-label="Uygulama ayarları"
            onClick={onOpenSettings}
          >
            <IconGear size={16} />
          </button>
        </div>
      </header>

      <div className="pv-body" ref={splitRef}>
        <div className="pv-left" id="pane-video" ref={videoPaneRef}>
          <div className="card pv-card">
            <header className="pv-card-head">
              <span className="card-title">Önizleme</span>
              <span
                className="pv-keys"
                title="Space/K: oynat·duraklat · ←/→: 5 sn (Shift: 1 sn) · J/L: 10 sn · ↑/↓: önceki/sonraki altyazı · M: sessiz · F: tam ekran · /: ara"
              >
                ⌨ kısayollar
              </span>
            </header>
            <div
              className={"video-box" + (playing ? " playing" : "") + (expanded ? " expanded" : "")}
              id="video-box"
            >
              <video
                ref={videoRef}
                controls
                playsInline
                aria-label="Video önizleme"
                aria-keyshortcuts="Space K ArrowLeft ArrowRight J L ArrowUp ArrowDown M F"
                onLoadedMetadata={onMeta}
                onPlay={() => setPlaying(true)}
                onPause={() => setPlaying(false)}
                onEnded={() => setPlaying(false)}
              />
              <button className="pv-play" title={playing ? "Duraklat (Space)" : "Oynat (Space)"}
                aria-label={playing ? "Duraklat" : "Oynat"}
                onClick={togglePlay}>
                <IconPlay size={22} />
              </button>
              <div className="caps" id="caps">
                {tracks.map((t) => (
                  <div key={t.key} className="cap" id={"cap-" + t.key} />
                ))}
              </div>
            </div>
          </div>

          <TranslateBar
            tracks={tracks}
            pending={pending}
            busy={translating}
            status={status}
            onToggle={togglePending}
            onTranslate={startTranslate}
            onRemove={removeTranslation}
            removing={removing}
            error={actionError}
          />

        </div>

        <div className="gutter" id="gutter" title="Sürükleyerek genişlik ayarla" />

        <div className="pv-right">
          <header className="pv-panel-head">
            <span className="card-title">Altyazılar</span>
            <span className="pv-count">{visible.length} satır</span>
          </header>
          <div className="track-chips">
            {tracks.length > 1 && (
              <button
                className={"all" + (view === "all" ? " act" : "")}
                onClick={() => {
                  setView("all");
                  setActiveKey(viewTracks[0]?.key ?? null);
                }}
              >
                Tümü
              </button>
            )}
            {viewTracks.map((t) => (
              <button
                key={t.key}
                className={t.key === activeKey ? "act" : ""}
                title={t.kind === "track" ? "kaynak dil" : "çeviri"}
                onClick={() => {
                  setActiveKey(t.key);
                  setView(t.key);
                }}
              >
                {t.kind === "track" ? `kaynak · ${langName(t.lang)}` : langName(t.lang)}
              </button>
            ))}
          </div>
          <div className="pv-toolbar">
            <div className="pv-search">
              <IconSearch size={14} />
              <input
                ref={searchRef}
                type="search"
                placeholder="Metin ara…"
                aria-label="Cue metninde ara"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              {query && (
                <button
                  className="pv-clear"
                  title="Temizle"
                  onClick={() => {
                    setQuery("");
                    searchRef.current?.focus();
                  }}
                >
                  ✕
                </button>
              )}
            </div>
            <button
              className={"icon-btn" + (onlyFlagged ? " act" : "")}
              title="Yalnızca işaretliler"
              aria-pressed={onlyFlagged}
              onClick={() => setOnlyFlagged((o) => !o)}
            >
              <IconFilter size={15} />
            </button>
          </div>
          <div className="cue-list" ref={cueListRef}>
            {visible.map(({ cue, index, flags }) => (
              <div
                key={cue.id}
                className="cue"
                data-cue-index={index}
                tabIndex={0}
                onClick={() => seek(cue.start_ms)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    seek(cue.start_ms);
                  }
                }}
              >
                <span className="cue-badge">
                  <span className="cue-num">{index + 1}</span>
                  <IconPlay size={10} className="cue-play" />
                </span>
                <div className="cue-main">
                  <div className="cue-top">
                    <span className="cue-time">
                      {fmt(cue.start_ms)} → {fmt(cue.end_ms)}
                    </span>
                    {flags.length > 0 && (
                      <span className="cue-flag">
                        <IconAlert size={11} />
                        {flags
                          .map((x) => (x.flag.type === "too_fast" ? `too_fast(${x.flag.cps?.toFixed(1)})` : x.flag.type))
                          .join(", ")}
                      </span>
                    )}
                  </div>
                  <div className="cue-text">{cue.text}</div>
                </div>
                <span className="cue-dur">{((cue.end_ms - cue.start_ms) / 1000).toFixed(1)}s</span>
              </div>
            ))}
          </div>
          {visible.length === 0 && <div className="empty pv-empty">Eşleşen satır yok.</div>}
        </div>
      </div>
      {activeTrack && (
        <footer className="standards">
          <span>profil {settings.profile}</span>
          <span className="sep">·</span>
          <span>
            {NETFLIX.maxChars} karakter × {NETFLIX.lines} satır
          </span>
          <span className="sep">·</span>
          <span>≤ {NETFLIX.maxCps} cps</span>
          <span className="sep">·</span>
          <span>min {NETFLIX.minMs} ms</span>
          <span className="sep">·</span>
          <span>ara ≥ {NETFLIX.gapMs} ms</span>
          <span className="sep">·</span>
          <span>{activeTrack.cues.length} cue</span>
          {flagged > 0 && (
            <>
              <span className="sep">·</span>
              <span className="warn">{flagged} işaretli</span>
            </>
          )}
        </footer>
      )}
      {exportOpen && (
        <ExportModal
          tracks={tracks}
          formats={formats}
          busy={exporting}
          error={actionError}
          onClose={() => setExportOpen(false)}
          onConfirm={(paths, format) => void runExport(paths, format)}
        />
      )}
      {deleteAsk && (
        <div id="delete-backdrop" onClick={() => !deleteBusy && setDeleteAsk(false)}>
          <div
            className="confirm-card"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="delete-title"
            onClick={(e) => e.stopPropagation()}
          >
            <h2 id="delete-title">Bu iş silinsin mi?</h2>
            <p>
              <b>{jobTitle(job)}</b> için üretilen tüm altyazılar ve dosyalar kalıcı olarak
              silinir. Bu işlem geri alınamaz.
            </p>
            <div className="confirm-actions">
              <button className="ghost" onClick={() => setDeleteAsk(false)} disabled={deleteBusy}>
                Vazgeç
              </button>
              <button className="ghost danger" onClick={() => void confirmDelete()} disabled={deleteBusy}>
                <IconTrash size={14} /> {deleteBusy ? "Siliniyor…" : "Evet, sil"}
              </button>
            </div>
          </div>
        </div>
      )}
      {expanded && <div id="expand-backdrop" onClick={() => setExpanded(false)} />}
    </section>
  );
}
