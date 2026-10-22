// Shown in place of the preview when no job is selected.
// Keys: / search, arrows move, Enter opens, Backspace deletes.
import { useEffect, useMemo, useRef, useState } from "react";
import type { JobRow } from "../types";
import {
  IconFolderPlus,
  IconGrid,
  IconList,
  IconRefresh,
  IconSearch,
  IconTrash,
} from "./icons";

type Layout = "list" | "grid";
type Sort = "recent" | "name";

interface LibraryViewProps {
  jobs: JobRow[];
  onOpen: (jobId: number) => void;
  onDelete: (job: JobRow) => void;
  onRetranscribe: (job: JobRow) => void;
  busy: number | null;
}

const LAYOUT_KEY = "sub0.library.layout";
const SORT_KEY = "sub0.library.sort";

// Folders are named after the video (`naming::job_dir`); legacy `job_NNN`
// folders fall back to the source ref.
function title(job: JobRow): string {
  const folder = job.out_dir.split(/[\\/]/).filter(Boolean).pop() ?? "";
  if (folder && !/^job_\d+$/.test(folder)) return folder;
  if (job.source_kind === "url") {
    try {
      const u = new URL(job.source_ref);
      const last = u.pathname.split("/").filter(Boolean).pop();
      return last ? decodeURIComponent(last) : u.hostname;
    } catch {
      return job.source_ref;
    }
  }
  return job.source_ref.split(/[\\/]/).filter(Boolean).pop() ?? job.source_ref;
}

function readStored<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return allowed.includes(v as T) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
}

export default function LibraryView({ jobs, onOpen, onDelete, onRetranscribe, busy }: LibraryViewProps) {
  const [layout, setLayout] = useState<Layout>(() => readStored(LAYOUT_KEY, ["grid", "list"], "grid"));
  const [sort, setSort] = useState<Sort>(() => readStored(SORT_KEY, ["recent", "name"], "recent"));
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);

  const done = useMemo(() => jobs.filter((j) => j.status === "done"), [jobs]);

  const shown = useMemo(() => {
    const q = query.trim().toLocaleLowerCase("tr");
    const hit = q
      ? done.filter((j) => title(j).toLocaleLowerCase("tr").includes(q))
      : done.slice();
    return sort === "name" ? hit.sort((a, b) => title(a).localeCompare(title(b), "tr")) : hit;
  }, [done, query, sort]);

  // Otherwise the cursor can point at a filtered-out card and Enter opens the wrong job.
  useEffect(() => setCursor(0), [query, sort, layout]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = document.activeElement;
      const typing = el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;
      if (e.key === "/" && !typing) {
        e.preventDefault();
        searchRef.current?.focus();
      }
      if (e.key === "Escape" && document.activeElement === searchRef.current) {
        setQuery("");
        searchRef.current?.blur();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  const columns = (gridRef.current?.dataset.columns ?? "1") as string;
  const onGridKey = (e: React.KeyboardEvent) => {
    const perRow = layout === "grid" ? Number(columns) || 1 : 1;
    if (e.key === "ArrowRight") setCursor((c) => Math.min(c + 1, shown.length - 1));
    else if (e.key === "ArrowLeft") setCursor((c) => Math.max(c - 1, 0));
    else if (e.key === "ArrowDown") setCursor((c) => Math.min(c + perRow, shown.length - 1));
    else if (e.key === "ArrowUp") setCursor((c) => Math.max(c - perRow, 0));
    else if (e.key === "Enter" && shown[cursor]) onOpen(shown[cursor].id);
    else if ((e.key === "Backspace" || e.key === "Delete") && shown[cursor]) onDelete(shown[cursor]);
    else return;
    e.preventDefault();
  };

  useEffect(() => {
    gridRef.current
      ?.querySelector<HTMLElement>(`[data-index="${cursor}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const change = (key: string, value: string) => {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Storage can be unavailable (private mode); not worth surfacing.
    }
  };

  const none = done.length === 0;

  return (
    <section id="view-library" className="view active">
      <header className="view-head">
        <div>
          <h1>Projeler</h1>
          <div className="view-sub">
            {none ? "Henüz tamamlanmış iş yok" : `${shown.length} / ${done.length} proje`}
          </div>
        </div>
        <div className="head-actions">
          <label className="sr-only" htmlFor="lib-sort">
            Sıralama
          </label>
          <select
            id="lib-sort"
            className="ctl text slim"
            value={sort}
            onChange={(e) => {
              setSort(e.target.value as Sort);
              change(SORT_KEY, e.target.value);
            }}
          >
            <option value="recent">En yeni</option>
            <option value="name">Ada göre</option>
          </select>
          <div className="seg" role="group" aria-label="Görünüm">
            <button
              className={layout === "grid" ? "on" : ""}
              aria-pressed={layout === "grid"}
              title="Kart görünümü"
              onClick={() => {
                setLayout("grid");
                change(LAYOUT_KEY, "grid");
              }}
            >
              <IconGrid size={14} />
            </button>
            <button
              className={layout === "list" ? "on" : ""}
              aria-pressed={layout === "list"}
              title="Liste görünümü"
              onClick={() => {
                setLayout("list");
                change(LAYOUT_KEY, "list");
              }}
            >
              <IconList size={14} />
            </button>
          </div>
        </div>
      </header>

      {!none && (
        <div className="lib-search">
          <IconSearch size={15} />
          <input
            ref={searchRef}
            type="search"
            placeholder="Proje ara…"
            aria-label="Proje ara"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {!query && <kbd className="lib-kbd">/</kbd>}
        </div>
      )}

      <div
        className={"library " + layout}
        ref={gridRef}
        data-columns={layout === "grid" ? 3 : 1}
        tabIndex={0}
        role="listbox"
        aria-label="Projeler"
        onKeyDown={onGridKey}
      >
        {shown.map((j, i) => (
          <article
            key={j.id}
            role="option"
            aria-selected={i === cursor}
            data-index={i}
            className={"lib-card" + (i === cursor ? " cur" : "")}
            // Entrance stagger caps at 8 so long lists don't crawl in.
            style={{ "--i": Math.min(i, 8) } as React.CSSProperties}
          >
            <button className="lib-open" onClick={() => onOpen(j.id)}>
              <span className="lib-title">{title(j)}</span>
              <span className="lib-meta">
                {jobTitleTooltip(j)}
              </span>
            </button>
            <div className="lib-actions">
              <button
                className="ghost icon"
                title="Yeniden transkript et"
                aria-label={`${title(j)} için yeniden transkript`}
                disabled={busy === j.id}
                onClick={() => onRetranscribe(j)}
              >
                <IconRefresh size={14} />
              </button>
              <button
                className="ghost icon danger"
                title="Projeyi sil"
                aria-label={`${title(j)} projesini sil`}
                disabled={busy === j.id}
                onClick={() => onDelete(j)}
              >
                <IconTrash size={14} />
              </button>
            </div>
          </article>
        ))}

        {shown.length === 0 && !none && (
          <div className="lib-none">
            <p>
              <b>{query}</b> için sonuç yok.
            </p>
            <button className="ghost" onClick={() => setQuery("")}>
              Aramayı temizle
            </button>
          </div>
        )}

        {none && (
          <div className="lib-none">
            <IconFolderPlus size={26} />
            <p>Tamamlanan işler burada toplanır.</p>
            <span className="hint">Stüdyo ekranından yeni bir video ekleyerek başla.</span>
          </div>
        )}
      </div>
    </section>
  );
}

function jobTitleTooltip(j: JobRow): string {
  return j.source_kind === "url" ? "Bağlantı · batch " + j.batch_id : "Yerel dosya · batch " + j.batch_id;
}
