/* Settings: the UI side of data/settings.toml. Saved settings apply to new
 * jobs (the backend is the source of truth). The fields map one to one onto
 * the RunConfig parameters in docs/cli-reference.md.
 * Except the appearance section: the theme never enters settings.toml, it is a
 * local UI preference. */
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { UiSettings } from "../types";
import { THEMES, type ThemeId } from "../theme";
import { bgRgb, inkRgb, type SubtitleStyle } from "../subtitleStyle";
import ModelPicker from "./ModelPicker";
import LanguageMultiSelect from "./LanguageMultiSelect";
import KeyField, { type KeySource } from "./KeyField";
import { PRESET_IDS, isSttModelValidFor, type ProviderId } from "../models";

interface StoredKeys {
  groq: string | null;
  openrouter: string | null;
}

interface SettingsViewProps {
  active: boolean;
  settings: UiSettings;
  onSave: (s: UiSettings) => void;
  theme: ThemeId;
  onThemeChange: (t: ThemeId) => void;
  /** Current subtitle appearance, shown live in the row preview. */
  subtitleStyle: SubtitleStyle;
  onEditSubtitleStyle: () => void;
  onReplayTutorial: () => void;
}

/** One settings row: title and description on the left, control on the right. */
function Row({
  title,
  desc,
  children,
}: {
  title: string;
  desc: string;
  children: React.ReactNode;
}) {
  return (
    <div className="row">
      <div className="row-text">
        <div className="row-title">{title}</div>
        <div className="row-desc">{desc}</div>
      </div>
      {children}
    </div>
  );
}

export default function SettingsView({
  active,
  settings,
  onSave,
  theme,
  onThemeChange,
  subtitleStyle,
  onEditSubtitleStyle,
  onReplayTutorial,
}: SettingsViewProps) {
  const pickOutputDir = async () => {
    // Starting point: opens at the current selection when it is a valid folder.
    const picked = await open({ directory: true, multiple: false, defaultPath: form.out_dir });
    if (typeof picked === "string") set("out_dir", picked);
  };

  const [status, setStatus] = useState("");
  const [statusErr, setStatusErr] = useState(false);
  const [form, setForm] = useState(settings);
  const [storedKeys, setStoredKeys] = useState<StoredKeys>({
    groq: null,
    openrouter: null,
  });
  const [keySources, setKeySources] = useState<Record<string, KeySource>>({
    GROQ_API_KEY: null,
    OPENROUTER_API_KEY: null,
  });

  /** Refresh the stored keys and their sources (after KeyField saved or deleted). */
  const reloadKeys = () => {
    void api
      .loadKeys()
      .then(setStoredKeys)
      .catch(() => setStoredKeys({ groq: null, openrouter: null }));
    void api
      .keysStatus()
      .then((s) => {
        const sources = s.sources ?? {};
        setKeySources({
          GROQ_API_KEY: sources.GROQ_API_KEY || null,
          OPENROUTER_API_KEY: sources.OPENROUTER_API_KEY || null,
        });
      })
      .catch(() =>
        setKeySources({ GROQ_API_KEY: null, OPENROUTER_API_KEY: null }),
      );
  };

  useEffect(() => {
    reloadKeys();
  }, []);

  const set = <K extends keyof UiSettings>(key: K, value: UiSettings[K]) =>
    setForm((f) => ({ ...f, [key]: value }));

  /**
  /** When the provider changes, an STT model id that does not belong to the new
  /** provider is remapped to that provider's first featured model. Otherwise
  /** Groq's bare slug reaches OpenRouter and comes back 400 (see
  /** `isSttModelValidFor`).
  /** */
  const setSttProvider = (provider: string) =>
    setForm((f) => ({
      ...f,
      stt_provider: provider,
      stt_model: isSttModelValidFor(provider, f.stt_model)
        ? f.stt_model
        : (PRESET_IDS.stt[provider as ProviderId]?.[0] ?? f.stt_model),
    }));

  const save = async () => {
    try {
      await api.saveSettings(form);
      onSave(form);
      setStatusErr(false);
      setStatus("Kaydedildi ✓. Yeni işlerde geçerli.");
      setTimeout(() => setStatus(""), 3000);
    } catch (e) {
      setStatusErr(true);
      setStatus("Hata: " + String(e));
    }
  };

  return (
    <section id="view-settings" className={"view" + (active ? " active" : "")}>
      <div className="settings-col">
        <header className="view-head">
          <h1>Ayarlar</h1>
          <div className="head-actions">
            <button
              className="ghost"
              title="data/settings.toml"
              onClick={async () => api.revealPath(await api.settingsPath())}
            >
              settings.toml
            </button>
            <button className="primary" onClick={save}>
              Kaydet
            </button>
          </div>
        </header>
        <div className="settings-scroll">
          <h2>Görünüm</h2>
          <div className="theme-grid" role="group" aria-label="Tema">
            {THEMES.map((t) => (
              <button
                key={t.id}
                type="button"
                className="theme-card"
                data-theme-id={t.id}
                aria-pressed={theme === t.id}
                onClick={() => onThemeChange(t.id)}
              >
                <span className="theme-swatch" aria-hidden="true">
                  {t.swatch.map((c) => (
                    <i key={c} style={{ background: c }} />
                  ))}
                </span>
                <span className="theme-name">{t.label}</span>
                <span className="theme-hint">{t.hint}</span>
              </button>
            ))}
          </div>

          <Row
            title="Altyazı stili"
            desc="Yazı ve arkaplan rengi ile arkaplan opaklığı. Yalnızca önizlemede görünür, dışa aktarılan SRT dosyalarını etkilemez."
          >
            <div className="style-row">
              <span
                className="style-row-preview"
                style={
                  {
                    "--cap-ink-rgb": inkRgb(subtitleStyle.ink),
                    "--cap-bg-rgb": bgRgb(subtitleStyle.bg),
                    "--cap-alpha": String(subtitleStyle.opacity / 100),
                    color: `rgb(${inkRgb(subtitleStyle.ink)} / ${subtitleStyle.opacity / 100})`,
                    background: `rgb(${bgRgb(subtitleStyle.bg)} / ${subtitleStyle.opacity / 100})`,
                  } as React.CSSProperties
                }
                aria-hidden="true"
              >
                Önizleme
              </span>
              <button type="button" className="style-row-btn" onClick={onEditSubtitleStyle}>
                Düzenle
              </button>
            </div>
          </Row>

          <h2>Genel</h2>
          <Row
            title="Çıktı klasörü"
            desc="İş klasörleri ve altyazı dosyaları buraya yazılır."
          >
            {/* The path is never typed: free text means a wrong folder sooner or later. A
              system picker fits the platform and also blocks choosing a path that does
              not exist. */}
            <div className="dir-picker">
              <span className="dir-path" title={form.out_dir}>
                {form.out_dir || "Klasör seçilmedi"}
              </span>
              <button type="button" className="secondary" onClick={pickOutputDir}>
                Gözat…
              </button>
            </div>
          </Row>
          <Row title="Profil" desc="Altyazı standartları profili.">
            <select
              className="ctl"
              value={form.profile}
              onChange={(e) => set("profile", e.target.value)}
            >
              <option value="netflix">netflix</option>
            </select>
          </Row>

          <h2>Sağlayıcılar</h2>
          <Row title="Kurulum rehberi" desc="Tanıtımı ve API anahtarı adımını yeniden aç.">
            <button type="button" className="ghost btn-sm" onClick={onReplayTutorial}>
              Rehberi aç
            </button>
          </Row>
          <Row title="Konuşma tanıma (STT)" desc="Sesi metne çeviren model.">
            <select
              className="ctl"
              value={form.stt_provider}
              onChange={(e) => setSttProvider(e.target.value)}
            >
              <option value="groq">Groq</option>
              <option value="openrouter">OpenRouter</option>
            </select>
          </Row>
          <Row title="STT modeli" desc="Fiyatlar ses saati başına.">
            <ModelPicker
              kind="stt"
              provider={form.stt_provider}
              value={form.stt_model}
              onChange={(v) => set("stt_model", v)}
            />
          </Row>
          <Row
            title="Dil modeli (LLM) sağlayıcı"
            desc="Çeviri ve dolgu temizliği için kullanılır."
          >
            <select
              className="ctl"
              value={form.llm_provider}
              onChange={(e) => set("llm_provider", e.target.value)}
            >
              <option value="groq">Groq</option>
              <option value="openrouter">OpenRouter</option>
            </select>
          </Row>
          <Row title="LLM modeli" desc="Fiyatlar 1 milyon token başına.">
            <ModelPicker
              kind="llm"
              provider={form.llm_provider}
              value={form.llm_model}
              onChange={(v) => set("llm_model", v)}
            />
          </Row>
          <Row
            title="Groq API anahtarı"
            desc="Sesi yazıya çevirmek için gerekir."
          >
            <KeyField
              provider="groq"
              label="Groq"
              stored={storedKeys.groq}
              source={keySources.GROQ_API_KEY}
              onSaved={reloadKeys}
            />
          </Row>
          <Row
            title="OpenRouter API anahtarı"
            desc="Çeviri ve temizlik için gerekir."
          >
            <KeyField
              provider="openrouter"
              label="OpenRouter"
              stored={storedKeys.openrouter}
              source={keySources.OPENROUTER_API_KEY}
              onSaved={reloadKeys}
            />
          </Row>

          <h2>Çeviri</h2>
          <Row
            title="Varsayılan hedef diller"
            desc="Yeni işler bu dillere çevrilir. Tıklayıp birden fazla dil seçebilirsiniz."
          >
            <LanguageMultiSelect
              selected={form.langs}
              onToggle={(code) =>
                set(
                  "langs",
                  form.langs.includes(code)
                    ? form.langs.filter((c) => c !== code)
                    : [...form.langs, code],
                )
              }
            />
          </Row>
          <Row
            title="Çeviri kalitesi"
            desc="Pro: içerik türünü ve kitlesini çıkarıp ona göre yerelleştirir (iş başına +1 istek). Hızlı: doğrudan toplu çeviri."
          >
            <select
              className="ctl"
              value={form.translation_mode}
              onChange={(e) => set("translation_mode", e.target.value as "fast" | "pro")}
            >
              <option value="pro">Pro (bağlama göre yerelleştirme)</option>
              <option value="fast">Hızlı</option>
            </select>
          </Row>
          <Row
            title="Üslup talimatı"
            desc="Çeviri istemine eklenen serbest metin. Boş bırakılabilir."
          >
            <textarea
              className="ctl"
              rows={2}
              value={form.translation_prompt}
              onChange={(e) => set("translation_prompt", e.target.value)}
            />
          </Row>

          <h2>Temizlik</h2>
          <Row
            title="Dolgu temizliği"
            desc="Model yalnızca kelime siler. Metin asla yeniden yazılmaz."
          >
            <label className="switch">
              <input
                type="checkbox"
                checked={form.cleanup}
                onChange={(e) => set("cleanup", e.target.checked)}
              />
              <span />
            </label>
          </Row>
          <Row
            title="Silme oranı üst sınırı"
            desc="Bir pencerede bu orandan fazla silme önerilirse pencere reddedilir."
          >
            <input
              type="number"
              className="ctl num"
              min={0.05}
              max={0.5}
              step={0.05}
              value={form.cleanup_ratio}
              onChange={(e) =>
                set("cleanup_ratio", parseFloat(e.target.value) || 0.2)
              }
            />
          </Row>
          <Row
            title="Baş/son kırpma (sn)"
            desc="Müzik girişi ve jenerikleri atmak için baştan ve sondan kırpma."
          >
            <div className="pair">
              <input
                type="number"
                className="ctl num"
                min={0}
                value={form.trim_start_secs}
                onChange={(e) =>
                  set("trim_start_secs", parseInt(e.target.value) || 0)
                }
              />
              <input
                type="number"
                className="ctl num"
                min={0}
                value={form.trim_end_secs}
                onChange={(e) =>
                  set("trim_end_secs", parseInt(e.target.value) || 0)
                }
              />
            </div>
          </Row>

          <h2>Gelişmiş</h2>
          <Row
            title="Chunk hedefi / tavanı (sn)"
            desc="Bölme sadece FLAC limiti veya süre tavanı aşılırsa devreye girer."
          >
            <div className="pair">
              <input
                type="number"
                className="ctl num"
                min={30}
                value={form.target_chunk_secs}
                onChange={(e) =>
                  set("target_chunk_secs", parseInt(e.target.value) || 600)
                }
              />
              <input
                type="number"
                className="ctl num"
                min={60}
                value={form.max_chunk_secs}
                onChange={(e) =>
                  set("max_chunk_secs", parseInt(e.target.value) || 900)
                }
              />
            </div>
          </Row>
          <Row
            title="FLAC yükleme limiti (MB)"
            desc="Groq ücretsiz planda 25 MB. Yaklaşık 2 MB/dakika konuşma."
          >
            <input
              type="number"
              className="ctl num"
              min={5}
              value={form.max_flac_mb}
              onChange={(e) =>
                set("max_flac_mb", parseInt(e.target.value) || 25)
              }
            />
          </Row>
          <Row
            title="Sessizlik eşiği"
            desc="Gürültü tabanı (dB) ve bölme için gereken en kısa sessizlik (sn)."
          >
            <div className="pair">
              <input
                type="text"
                className="ctl text"
                style={{ width: 90 }}
                value={form.noise_db}
                onChange={(e) => set("noise_db", e.target.value)}
              />
              <input
                type="number"
                className="ctl num"
                min={0.1}
                step={0.1}
                value={form.min_silence_secs}
                onChange={(e) =>
                  set("min_silence_secs", parseFloat(e.target.value) || 0.4)
                }
              />
            </div>
          </Row>
          <div className="save-line">
            <span style={statusErr ? { color: "var(--warn)" } : undefined}>
              {status}
            </span>
          </div>
        </div>
      </div>
    </section>
  );
}
