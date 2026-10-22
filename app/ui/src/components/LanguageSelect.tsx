import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import { filterLanguages, useLanguages } from "../languages";

interface LanguageSelectProps {
  selected: string[];
  /** Already translated; shown but not selectable. */
  locked?: string[];
  onToggle: (code: string) => void;
  autoFocus?: boolean;
  footer?: ReactNode;
}

export default function LanguageSelect({
  selected,
  locked = [],
  onToggle,
  autoFocus,
  footer,
}: LanguageSelectProps) {
  const all = useLanguages();
  const [filter, setFilter] = useState("");
  const rows = useMemo(() => filterLanguages(all, filter), [all, filter]);

  return (
    <>
      <div className="lang-search">
        <svg viewBox="0 0 24 24" className="bicon">
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.5-3.5" />
        </svg>
        <input
          autoFocus={autoFocus}
          placeholder="Dil ara…"
          aria-label="Dil ara"
          spellCheck={false}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>
      <div className="lang-grid">
        {rows.map((l) => {
          const isLocked = locked.includes(l.code);
          return (
            <label
              key={l.code}
              className={"lang-opt" + (selected.includes(l.code) ? " sel" : "") + (isLocked ? " locked" : "")}
              title={isLocked ? "bu dil zaten çevrili" : undefined}
            >
              <input
                type="checkbox"
                checked={selected.includes(l.code)}
                disabled={isLocked}
                onChange={() => onToggle(l.code)}
              />
              <span className="box" aria-hidden="true" />
              <span className="name">
                {l.name}
                <span className="muted"> · {l.native}</span>
              </span>
              <span className="code">{l.code}</span>
            </label>
          );
        })}
        {rows.length === 0 && <div className="lang-opt no-results">sonuç yok</div>}
      </div>
      <div className="lang-foot">
        <span className="hint">
          {filter.trim() ? `${rows.length} / ${all.length}` : `${all.length} dil`}
        </span>
        <span className="spacer" />
        {footer}
      </div>
    </>
  );
}
