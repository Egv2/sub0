// First-run tutorial: copy on the left, a small animated scene on the right.
// The last step collects the API keys; leaving without a Groq key asks first,
// because no job can run without one.
import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { api } from "../api";
import KeyField from "./KeyField";
import { PROVIDER_NAME, missingKeys, type KeyStatus } from "../useKeyStatus";
import { IconAlert, IconArrowLeft, IconArrowRight, IconCheck, IconX } from "./icons";

interface OnboardingProps {
  keys: KeyStatus;
  sttProvider: string;
  llmProvider: string;
  onKeysChanged: () => void;
  onDone: () => void;
  /** Start on the key step (e.g. opened from a "no key" warning). */
  startAtKeys?: boolean;
  /** Design harness only. */
  initialStep?: number;
}

interface Step {
  title: string;
  body: string;
  scene: "drop" | "transcribe" | "translate" | "keys";
}

const STEPS: Step[] = [
  {
    title: "Videonu bırak, altyazın hazır olsun.",
    body: "Bir dosya sürükle ya da bağlantı yapıştır. Sub0 sesi yazıya döker ve yayın standardında altyazı üretir.",
    scene: "drop",
  },
  {
    title: "Konuşma, temiz satırlara dönüşür.",
    body: "Gereksiz sesler, tekrarlar ve dolgu kelimeleri ayıklanır. Satırlar okunabilir hızda, doğru zamanlamayla bölünür.",
    scene: "transcribe",
  },
  {
    title: "İstediğin dile çevir, yan yana izle.",
    body: "Çeviri kelime kelime değil, içeriğin tonuna göre yapılır. Önizlemede dilleri değiştirip SRT ya da VTT olarak dışa aktar.",
    scene: "translate",
  },
  {
    title: "Son adım: API anahtarların.",
    body: "Sub0 kendi anahtarlarınla çalışır. Varsayılan olarak Groq sesi yazıya döker, OpenRouter çeviri ve temizliği yapar. Anahtarlar yalnızca bu bilgisayarda saklanır.",
    scene: "keys",
  },
];

const KEY_STEP = STEPS.length - 1;
const EASE = [0.16, 1, 0.3, 1] as const;

export default function Onboarding({
  keys,
  sttProvider,
  llmProvider,
  onKeysChanged,
  onDone,
  startAtKeys,
  initialStep,
}: OnboardingProps) {
  const missing = missingKeys(keys, sttProvider, llmProvider);
  const role = (p: "groq" | "openrouter") => {
    const stt = sttProvider === p;
    const llm = llmProvider === p;
    if (stt && llm) return "Zorunlu. Ses tanıma, çeviri ve temizlik.";
    if (stt) return "Zorunlu. Sesi yazıya çevirir.";
    if (llm) return "Çeviri ve akıllı temizlik için.";
    return "İsteğe bağlı. Ayarlar'dan sağlayıcı olarak seçilebilir.";
  };
  const [step, setStep] = useState(startAtKeys ? KEY_STEP : (initialStep ?? 0));
  const [dir, setDir] = useState(1);
  const [confirm, setConfirm] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);
  const reduce = useReducedMotion();
  const primaryRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  const go = useCallback((to: number) => {
    setConfirm(false);
    setDir(to > step ? 1 : -1);
    setStep(Math.max(0, Math.min(KEY_STEP, to)));
  }, [step]);

  // Leaving without the keys the selected providers need asks first.
  const leave = useCallback(() => {
    if (!missing.stt && !missing.llm) onDone();
    else setConfirm(true);
  }, [missing.stt, missing.llm, onDone]);

  useEffect(() => {
    (confirm ? confirmRef : primaryRef).current?.focus();
  }, [step, confirm]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) {
        if (e.key === "Escape") t.blur();
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        if (confirm) setConfirm(false);
        else leave();
      } else if (e.key === "ArrowRight" && step < KEY_STEP) go(step + 1);
      else if (e.key === "ArrowLeft" && step > 0) go(step - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [confirm, go, leave, step]);

  const openPage = (p: "groq" | "openrouter") => {
    setOpenError(null);
    api.openKeyPage(p).catch((e) => setOpenError(String(e)));
  };

  const s = STEPS[step];
  const last = step === KEY_STEP;
  const variants = {
    enter: (d: number) => (reduce ? { opacity: 0 } : { opacity: 0, x: d * 24 }),
    center: { opacity: 1, x: 0 },
    exit: (d: number) => (reduce ? { opacity: 0 } : { opacity: 0, x: d * -24 }),
  };

  return (
    <div className="ob-backdrop">
      <div className="ob" role="dialog" aria-modal="true" aria-labelledby="ob-title">
        <div className="ob-copy">
          <div className="ob-top">
            <span className="ob-brand">Sub0</span>
            <button className="ghost btn-sm ob-skip" onClick={leave}>
              {last ? "Kapat" : "Atla"}
            </button>
          </div>

          <AnimatePresence mode="wait" custom={dir} initial={false}>
            <motion.div
              key={step}
              className="ob-text"
              custom={dir}
              variants={variants}
              initial="enter"
              animate="center"
              exit="exit"
              transition={{ duration: reduce ? 0.12 : 0.32, ease: EASE }}
            >
              <h1 id="ob-title">{s.title}</h1>
              <p>{s.body}</p>

              {last && (
                <div className="ob-keys">
                  <KeyBlock
                    name="Groq"
                    need={role("groq")}
                    ok={keys.groq}
                    onOpen={() => openPage("groq")}
                  >
                    <KeyField
                      provider="groq"
                      label="Groq"
                      stored={keys.stored.groq}
                      source={keys.sources.groq}
                      onSaved={onKeysChanged}
                    />
                  </KeyBlock>
                  <KeyBlock
                    name="OpenRouter"
                    need={role("openrouter")}
                    ok={keys.openrouter}
                    onOpen={() => openPage("openrouter")}
                  >
                    <KeyField
                      provider="openrouter"
                      label="OpenRouter"
                      stored={keys.stored.openrouter}
                      source={keys.sources.openrouter}
                      onSaved={onKeysChanged}
                    />
                  </KeyBlock>
                  {openError && <p className="ob-err">Sayfa açılamadı: {openError}</p>}
                </div>
              )}
            </motion.div>
          </AnimatePresence>

          {confirm ? (
            <div className="ob-confirm" role="alert">
              <IconAlert size={18} />
              <div>
                <strong>
                  {missing.stt
                    ? `${PROVIDER_NAME[missing.stt]} anahtarı olmadan altyazı üretilemez.`
                    : `${PROVIDER_NAME[missing.llm ?? "openrouter"]} anahtarı eksik.`}
                </strong>
                <span>
                  {missing.stt
                    ? "İşler kuyruğa alınır ama başlatılamaz. Anahtarı sonra Ayarlar'dan da ekleyebilirsin."
                    : "Transkript çalışır, ancak çeviri ve akıllı temizlik kapalı kalır."}
                </span>
                <div className="ob-confirm-actions">
                  <button
                    ref={confirmRef}
                    className="primary btn-sm"
                    onClick={() => (last ? setConfirm(false) : go(KEY_STEP))}
                  >
                    Anahtarı ekle
                  </button>
                  <button className="ghost btn-sm" onClick={onDone}>
                    Yine de geç
                  </button>
                </div>
              </div>
            </div>
          ) : (
            <div className="ob-nav">
              <div className="ob-dots" role="tablist" aria-label="Tanıtım adımları">
                {STEPS.map((x, i) => (
                  <button
                    key={i}
                    role="tab"
                    aria-selected={i === step}
                    aria-label={`Adım ${i + 1}: ${x.title}`}
                    className={"ob-dot" + (i === step ? " on" : "")}
                    onClick={() => go(i)}
                  />
                ))}
              </div>
              <div className="ob-buttons">
                {step > 0 && (
                  <button className="ghost icon" aria-label="Geri" onClick={() => go(step - 1)}>
                    <IconArrowLeft size={16} />
                  </button>
                )}
                <button
                  ref={primaryRef}
                  className="primary"
                  onClick={() => (last ? leave() : go(step + 1))}
                >
                  {last ? "Başla" : "Devam"}
                  {!last && <IconArrowRight size={15} />}
                </button>
              </div>
            </div>
          )}
        </div>

        <div className="ob-stage" aria-hidden="true">
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={s.scene}
              className="ob-scene-wrap"
              initial={{ opacity: 0, scale: reduce ? 1 : 0.97 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: reduce ? 1 : 1.02 }}
              transition={{ duration: reduce ? 0.12 : 0.4, ease: EASE }}
            >
              <Scene kind={s.scene} keys={keys} />
            </motion.div>
          </AnimatePresence>
        </div>
      </div>
    </div>
  );
}

function KeyBlock({
  name,
  need,
  ok,
  onOpen,
  children,
}: {
  name: string;
  need: string;
  ok: boolean | null;
  onOpen: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="ob-key">
      <div className="ob-key-head">
        <span className={"ob-key-state" + (ok ? " ok" : "")}>
          {ok ? <IconCheck size={13} /> : <IconX size={13} />}
        </span>
        <div className="ob-key-name">
          <strong>{name}</strong>
          <span>{need}</span>
        </div>
        <button type="button" className="ghost btn-sm" onClick={onOpen}>
          Anahtar al
        </button>
      </div>
      {children}
    </div>
  );
}

/* Scenes: a mini version of the app's own surfaces, animated with CSS so the
   loop never touches React state. Colors come from theme variables. */
function Scene({ kind, keys }: { kind: Step["scene"]; keys: KeyStatus }) {
  if (kind === "drop") {
    return (
      <div className="scn scn-drop">
        <div className="scn-zone">
          <div className="scn-file">
            <span className="scn-file-ext">MP4</span>
            <span className="scn-file-name">roportaj.mp4</span>
          </div>
          <span className="scn-zone-label">Dosyayı buraya bırak</span>
        </div>
        <div className="scn-job">
          <span className="scn-job-name">roportaj.mp4</span>
          <span className="scn-bar"><i /></span>
        </div>
      </div>
    );
  }
  if (kind === "transcribe") {
    const lines = [
      ["00:00:01,200", "Bugün sizlere yeni projemizden"],
      ["00:00:03,900", "bahsetmek istiyorum."],
      ["00:00:06,100", "İlk sonuçlar beklediğimizden iyi."],
    ];
    return (
      <div className="scn scn-tx">
        <div className="scn-wave">
          {Array.from({ length: 28 }, (_, i) => (
            <i key={i} style={{ ["--i" as string]: i }} />
          ))}
        </div>
        <div className="scn-cues">
          {lines.map(([t, x], i) => (
            <div key={i} className="scn-cue" style={{ ["--i" as string]: i }}>
              <span className="scn-cue-n">{i + 1}</span>
              <div>
                <span className="scn-cue-t">{t}</span>
                <span className="scn-cue-x">{x}</span>
              </div>
            </div>
          ))}
        </div>
      </div>
    );
  }
  if (kind === "translate") {
    const caps = [
      ["TR", "İlk sonuçlar beklediğimizden iyi."],
      ["EN", "The first results beat our expectations."],
      ["DE", "Die ersten Ergebnisse übertreffen unsere Erwartungen."],
    ];
    return (
      <div className="scn scn-tr">
        <div className="scn-video">
          <div className="scn-video-glow" />
          {caps.map(([l, x], i) => (
            <span key={l} className="scn-cap" style={{ ["--i" as string]: i }}>
              {x}
            </span>
          ))}
        </div>
        <div className="scn-chips">
          {caps.map(([l], i) => (
            <span key={l} className="scn-chip" style={{ ["--i" as string]: i }}>
              {l}
            </span>
          ))}
        </div>
      </div>
    );
  }
  return (
    <div className="scn scn-keys">
      {(
        [
          ["Groq", "Ses → metin", keys.groq],
          ["OpenRouter", "Çeviri ve temizlik", keys.openrouter],
        ] as const
      ).map(([n, d, ok]) => (
        <div key={n} className={"scn-key" + (ok ? " ok" : "")}>
          <span className="scn-key-icon">{ok ? <IconCheck size={16} /> : <span className="scn-key-dash" />}</span>
          <div>
            <strong>{n}</strong>
            <span>{d}</span>
          </div>
          <span className="scn-key-state">{ok ? "Hazır" : "Bekleniyor"}</span>
        </div>
      ))}
      <p className="scn-keys-note">Anahtarlar bilgisayarından dışarı çıkmaz, yalnızca sağlayıcıya gönderilir.</p>
    </div>
  );
}
