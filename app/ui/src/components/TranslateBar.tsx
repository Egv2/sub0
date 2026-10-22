import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { UiTrack } from "./Preview";
import { langName, useLanguages } from "../languages";
import LanguagePicker from "./LanguagePicker";
import { usePopoverPosition } from "../usePopoverSide";

interface TranslateBarProps {
  tracks: UiTrack[];
  pending: string[];
  busy: string[];
  status: string;
  onToggle: (code: string) => void;
  onTranslate: () => void;
  onRemove: (code: string) => void;
  removing: string[];
  error: string | null;
}

export default function TranslateBar({
  tracks,
  pending,
  busy,
  status,
  onToggle,
  onTranslate,
  onRemove,
  removing,
  error,
}: TranslateBarProps) {
  useLanguages();
  const [open, setOpen] = useState(false);
  const popRef = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const pos = usePopoverPosition(open, btnRef);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      const t = e.target as Node;
      if (popRef.current?.contains(t) || btnRef.current?.contains(t)) return;
      setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("click", close);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("click", close);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  const source = tracks.find((t) => t.kind === "track");
  const done = tracks.filter((t) => t.kind === "translation");
  const existing = tracks.map((t) => t.lang);

  return (
    <div className="card tl-card">
      <div className="tl-row">
        <div className="tl-label">Çeviri</div>
        <div className="tl-chips">
          {/* The source track can't be removed, so it gets no delete button. */}
          {source && (
            <span className="chip source" title="STT ile çıkarılan orijinal transcript">
              kaynak · {langName(source.lang)}
            </span>
          )}
          {done.map((t) => (
            <span key={t.key} className="chip">
              {langName(t.lang)} ✓
              <button
                className="x"
                title="Bu çeviriyi sil"
                aria-label={`${langName(t.lang)} çevirisini sil`}
                disabled={removing.includes(t.lang)}
                onClick={() => onRemove(t.lang)}
              >
                {removing.includes(t.lang) ? "…" : "✕"}
              </button>
            </span>
          ))}
          {busy.map((code) => (
            <span key={"b-" + code} className="chip busy">
              {langName(code)}
            </span>
          ))}
          {pending
            .filter((code) => !busy.includes(code))
            .map((code) => (
              <span key={"p-" + code} className="chip pending">
                {langName(code)}
                <button className="x" title="çıkar" onClick={() => onToggle(code)}>
                  ✕
                </button>
              </span>
            ))}
        </div>
        <div className="tl-actions">
          <span className={"hint" + (error ? " error" : "")}>{error ?? status}</span>
          <button ref={btnRef} className="secondary" id="btn-add-lang" onClick={() => setOpen((o) => !o)}>
            <svg viewBox="0 0 24 24" className="bicon">
              <path d="M12 5v14M5 12h14" />
            </svg>
            Dil ekle
          </button>
        </div>
      </div>
      {/* Portaled so the preview panel's overflow doesn't clip it. */}
      {!open || !pos ? null : createPortal(
        <div className="lang-pop" data-side={pos.side} style={pos.style} ref={popRef}>
          <LanguagePicker
            pending={pending}
            existing={existing}
            onToggle={onToggle}
            onTranslate={() => {
              onTranslate();
              setOpen(false);
            }}
          />
        </div>,
        document.body,
      )}
    </div>
  );
}
