import { useEffect, useState } from "react";
import {
  BG_SWATCHES,
  INK_SWATCHES,
  MAX_OPACITY,
  MIN_OPACITY,
  type SubtitleStyle,
} from "../subtitleStyle";
import { IconX } from "./icons";

interface SubtitleStyleModalProps {
  style: SubtitleStyle;
  onApply: (style: SubtitleStyle) => void;
  onClose: () => void;
}

export default function SubtitleStyleModal({ style, onApply, onClose }: SubtitleStyleModalProps) {
  // Local draft so Cancel discards; only Apply commits.
  const [draft, setDraft] = useState<SubtitleStyle>(style);

  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", esc);
    return () => document.removeEventListener("keydown", esc);
  }, [onClose]);

  const ink = INK_SWATCHES.find((s) => s.id === draft.ink) ?? INK_SWATCHES[0];
  const bg = BG_SWATCHES.find((s) => s.id === draft.bg) ?? BG_SWATCHES[0];

  return (
    <div
      id="style-backdrop"
      // Backdrop click cancels.
      onClick={onClose}
      role="presentation"
    >
      <div
        className="style-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="style-title"
        onClick={(e) => e.stopPropagation()}
        style={
          {
            "--cap-ink-rgb": ink.rgb,
            "--cap-bg-rgb": bg.rgb,
            "--cap-alpha": String(draft.opacity / 100),
          } as React.CSSProperties
        }
      >
        <header className="style-head">
          <div>
            <h2 id="style-title">Altyazı stili</h2>
            <p>Yalnızca önizlemede görünür. Dışa aktarılan SRT dosyaları biçimsizdir.</p>
          </div>
          <button className="icon-btn" onClick={onClose} aria-label="Kapat">
            <IconX size={16} />
          </button>
        </header>

        <div className="style-preview" aria-label="Önizleme">
          <span className="cap on">
            O yüzden hatayı yaptıysam kusura bakmayın.
          </span>
        </div>

        <section className="style-block">
          <div className="style-label">
            <span>Yazı rengi</span>
            <span className="style-current">{ink.label}</span>
          </div>
          <div className="swatches" role="radiogroup" aria-label="Yazı rengi">
            {INK_SWATCHES.map((s) => (
              <button
                key={s.id}
                role="radio"
                aria-checked={draft.ink === s.id}
                className={"swatch ink" + (draft.ink === s.id ? " on" : "")}
                style={{ background: `rgb(${s.rgb})` }}
                title={s.label}
                onClick={() => setDraft((d) => ({ ...d, ink: s.id }))}
              >
                <span className="sr-only">{s.label}</span>
              </button>
            ))}
          </div>
        </section>

        <section className="style-block">
          <div className="style-label">
            <span>Arkaplan rengi</span>
            <span className="style-current">{bg.label}</span>
          </div>
          <div className="swatches" role="radiogroup" aria-label="Arkaplan rengi">
            {BG_SWATCHES.map((s) => (
              <button
                key={s.id}
                role="radio"
                aria-checked={draft.bg === s.id}
                className={"swatch bg" + (draft.bg === s.id ? " on" : "")}
                style={{ background: `rgb(${s.rgb})` }}
                title={s.label}
                onClick={() => setDraft((d) => ({ ...d, bg: s.id }))}
              >
                <span className="sr-only">{s.label}</span>
              </button>
            ))}
          </div>
        </section>

        <section className="style-block">
          <div className="style-label">
            <span>Arkaplan opaklığı</span>
            <span className="style-current">%{draft.opacity}</span>
          </div>
          <input
            className="style-range"
            type="range"
            min={MIN_OPACITY}
            max={MAX_OPACITY}
            step={2}
            value={draft.opacity}
            aria-label="Arkaplan opaklığı"
            onChange={(e) => setDraft((d) => ({ ...d, opacity: Number(e.target.value) }))}
          />
          <p className="style-hint">
            Düşük opaklıkta yazı arka planı kaybeder ve okunmaz olur; en alt değer
            okunabilirliği korur.
          </p>
        </section>

        <footer className="style-foot">
          <button className="ghost" onClick={onClose}>
            Vazgeç
          </button>
          <button className="primary" onClick={() => onApply(draft)}>
            Uygula
          </button>
        </footer>
      </div>
    </div>
  );
}
