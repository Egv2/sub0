import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { KeySource } from "./components/KeyField";

export interface KeyStatus {
  /** null until the first check finishes. */
  groq: boolean | null;
  openrouter: boolean | null;
  stored: { groq: string | null; openrouter: string | null };
  sources: { groq: KeySource; openrouter: KeySource };
}

const EMPTY: KeyStatus = {
  groq: null,
  openrouter: null,
  stored: { groq: null, openrouter: null },
  sources: { groq: null, openrouter: null },
};

/** Which provider keys are usable (file or environment). `reload` after saving. */
export function useKeyStatus() {
  const [status, setStatus] = useState<KeyStatus>(EMPTY);

  const reload = useCallback(async () => {
    const [s, stored] = await Promise.all([
      api.keysStatus().catch(() => null),
      api.loadKeys().catch(() => ({ groq: null, openrouter: null })),
    ]);
    setStatus({
      groq: s ? s.GROQ_API_KEY : false,
      openrouter: s ? s.OPENROUTER_API_KEY : false,
      stored,
      sources: {
        groq: s?.sources?.GROQ_API_KEY || null,
        openrouter: s?.sources?.OPENROUTER_API_KEY || null,
      },
    });
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  return { status, reload };
}

export type Provider = "groq" | "openrouter";
export const PROVIDER_NAME: Record<Provider, string> = { groq: "Groq", openrouter: "OpenRouter" };

const asProvider = (p: string): Provider => (p === "openrouter" ? "openrouter" : "groq");

/**
 * The key the selected speech-to-text provider needs (required for any job) and
 * the one the LLM provider needs (translation, smart cleanup). null = present.
 */
export function missingKeys(
  status: KeyStatus,
  sttProvider: string,
  llmProvider: string,
): { stt: Provider | null; llm: Provider | null } {
  const stt = asProvider(sttProvider);
  const llm = asProvider(llmProvider);
  return {
    stt: status[stt] === false ? stt : null,
    llm: status[llm] === false ? llm : null,
  };
}
