import { useEffect, useState } from "react";
import { api } from "../api";

export type KeyProvider = "groq" | "openrouter";
export type KeySource = "file" | "env" | null;

interface KeyFieldProps {
  provider: KeyProvider;
  label: string;
  stored: string | null;
  /** Where the effective key comes from. */
  source: KeySource;
  onSaved: () => void;
}

const PLACEHOLDER: Record<KeyProvider, string> = {
  groq: "gsk_…",
  openrouter: "sk-or-v1-…",
};

const SOURCE_LABEL: Record<Exclude<KeySource, null>, string> = {
  file: "Anahtar data/keys.json dosyasında.",
  env: "Anahtar ortam değişkeninden geliyor. Buraya yazarsan dosyadaki değer onu ezer.",
};

export default function KeyField({ provider, label, stored, source, onSaved }: KeyFieldProps) {
  const [draft, setDraft] = useState("");
  const [revealed, setRevealed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [error, setError] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  useEffect(() => {
    if (!confirmClear) return;
    const t = setTimeout(() => setConfirmClear(false), 3000);
    return () => clearTimeout(t);
  }, [confirmClear]);

  const patch = (value: string | null) =>
    provider === "groq" ? { groq: value } : { openrouter: value };

  const save = async () => {
    setBusy(true);
    try {
      await api.saveKeys(patch(draft));
      setDraft("");
      setRevealed(false);
      setError(false);
      setStatus("Kaydedildi");
      onSaved();
    } catch (e) {
      setError(true);
      setStatus("Hata: " + String(e));
    } finally {
      setBusy(false);
    }
  };

  const clear = async () => {
    if (!confirmClear) {
      setConfirmClear(true);
      return;
    }
    setBusy(true);
    try {
      await api.saveKeys(patch(null));
      setError(false);
      setStatus("Silindi");
      setConfirmClear(false);
      onSaved();
    } catch (e) {
      setError(true);
      setStatus("Hata: " + String(e));
    } finally {
      setBusy(false);
    }
  };

  const idle = stored ? (source ? SOURCE_LABEL[source] : "Kayıtlı anahtar var.") : "Henüz kayıtlı anahtar yok.";

  return (
    <div>
      <div className="pair" style={{ width: "100%" }}>
        <input
          className="ctl text"
          style={{ flex: 1, minWidth: 0, width: "auto" }}
          type={revealed ? "text" : "password"}
          autoComplete="off"
          spellCheck={false}
          placeholder={stored ? `••••••••${stored.slice(-4)}` : PLACEHOLDER[provider]}
          aria-label={`${label} API anahtarı`}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <button
          type="button"
          className="ghost btn-sm"
          aria-pressed={revealed}
          onClick={() => setRevealed((r) => !r)}
        >
          {revealed ? "Gizle" : "Göster"}
        </button>
        <button type="button" className="btn-sm" disabled={!draft.trim() || busy} onClick={save}>
          Kaydet
        </button>
        {stored && (
          <button
            type="button"
            className="btn-sm"
            disabled={busy}
            onClick={clear}
            style={confirmClear ? { color: "var(--warn)" } : undefined}
          >
            {confirmClear ? "Emin misin?" : "Sil"}
          </button>
        )}
      </div>
      <div
        className="hint"
        aria-live="polite"
        style={{ marginTop: 6, color: error ? "var(--warn)" : undefined }}
      >
        {status || idle}
      </div>
    </div>
  );
}
