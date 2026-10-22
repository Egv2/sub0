import { useEffect, useState } from "react";
import { IconDownload, IconX } from "./icons";
import type { UiTrack } from "./Preview";

export interface ExportModalProps {
  tracks: UiTrack[];
  formats: string[];
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onConfirm: (paths: string[], format: string) => void;
}

const FORMAT_LABELS: Record<string, string> = {
  srt: "SRT",
  vtt: "VTT",
  txt: "TXT",
};

export default function ExportModal({
  tracks,
  formats,
  busy,
  error,
  onClose,
  onConfirm,
}: ExportModalProps) {
  const [format, setFormat] = useState(formats[0] ?? "srt");
  const [picked, setPicked] = useState<string[]>(() =>
    tracks.map((t) => t.path),
  );

  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && !busy && onClose();
    document.addEventListener("keydown", esc);
    return () => document.removeEventListener("keydown", esc);
  }, [busy, onClose]);

  const toggle = (path: string) =>
    setPicked((p) =>
      p.includes(path) ? p.filter((x) => x !== path) : [...p, path],
    );

  const all = tracks.length > 0 && picked.length === tracks.length;
  const none = picked.length === 0;

  return (
    <div id="export-backdrop" onClick={() => !busy && onClose()}>
      <div
        className="export-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="export-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="export-head">
          <h2 id="export-title">Dışa aktar</h2>
          <button
            className="icon-btn"
            onClick={onClose}
            disabled={busy}
            aria-label="Kapat"
          >
            <IconX size={16} />
          </button>
        </header>

        <section className="export-block">
          <div className="export-label">Biçim</div>
          <div
            className="export-formats"
            role="radiogroup"
            aria-label="Dışa aktarım biçimi"
          >
            {formats.map((f) => (
              <button
                key={f}
                role="radio"
                aria-checked={format === f}
                className={"export-fmt" + (format === f ? " on" : "")}
                onClick={() => setFormat(f)}
                disabled={busy}
              >
                {FORMAT_LABELS[f] ?? f.toUpperCase()}
              </button>
            ))}
          </div>
        </section>

        {tracks.length > 1 && (
          <section className="export-block">
            <div className="export-label">
              Diller
              <button
                className="export-all"
                onClick={() => setPicked(all ? [] : tracks.map((t) => t.path))}
                disabled={busy}
              >
                {all ? "Seçimi kaldır" : "Tümünü seç"}
              </button>
            </div>
            <div className="export-langs">
              {tracks.map((t) => (
                <label key={t.key} className="export-lang">
                  <input
                    type="checkbox"
                    checked={picked.includes(t.path)}
                    onChange={() => toggle(t.path)}
                    disabled={busy}
                  />
                  <span className="box" aria-hidden="true" />
                  <span className="name">{t.label}</span>
                </label>
              ))}
            </div>
          </section>
        )}

        {error && <div className="export-error">{error}</div>}

        <footer className="export-foot">
          <span className="hint">
            {tracks.length === 1
              ? "1 dosya"
              : `${picked.length} / ${tracks.length} dosya`}
          </span>
          <button
            className="primary"
            disabled={none || busy}
            onClick={() => onConfirm(picked, format)}
          >
            <IconDownload size={14} /> {busy ? "Hazırlanıyor…" : "Dışa aktar"}
          </button>
        </footer>
      </div>
    </div>
  );
}
