// Prices and context sizes come from the `model_catalog` command. Groq has no
// pricing API, so its prices are a dated backend overlay ("static").

export type ProviderId = "groq" | "openrouter";
export type ModelKind = "stt" | "llm";
export type PricingSource = "live" | "static" | "none";

export interface CatalogEntry {
  id: string;
  name: string;
  /** 0 when unknown. */
  context: number;
  /** USD per 1M tokens. */
  in_per_m: number | null;
  out_per_m: number | null;
  /** USD per hour, for STT models billed by duration. */
  per_hour: number | null;
  pricing: PricingSource;
}

export interface CatalogProvider {
  live: boolean;
  error: string | null;
  llm: CatalogEntry[];
  stt: CatalogEntry[];
}

export interface ModelCatalog {
  fetched_at: string;
  cached: boolean;
  groq_prices_checked_at?: string;
  providers: Partial<Record<ProviderId, CatalogProvider>>;
}

export const PRESET_IDS: Record<ModelKind, Record<ProviderId, string[]>> = {
  stt: {
    groq: ["whisper-large-v3", "whisper-large-v3-turbo"],
    openrouter: [
      "openai/whisper-large-v3",
      "openai/whisper-large-v3-turbo",
      "openai/whisper-1",
      "openai/gpt-4o-transcribe",
    ],
  },
  llm: {
    // Groq dropped the llama models from its list (2026-10-15).
    groq: ["openai/gpt-oss-20b", "openai/gpt-oss-120b", "qwen/qwen3.8-27b"],
    openrouter: [
      "google/gemma-4-26b-a4b-it",
      "qwen/qwen3.8-27b",
      "qwen/qwen3-30b-a3b",
      "deepseek/deepseek-v4-flash-0731",
      "deepseek/deepseek-v4.1-flash",
      "z-ai/glm-5.3-flash",
      "z-ai/glm-4.7-flash",
      "openrouter/free",
    ],
  },
};

export function entriesFor(
  catalog: ModelCatalog | null,
  kind: ModelKind,
  provider: string,
): CatalogEntry[] {
  const p = catalog?.providers?.[provider as ProviderId];
  if (!p) return [];
  return kind === "stt" ? p.stt : p.llm;
}

export function findEntry(
  catalog: ModelCatalog | null,
  kind: ModelKind,
  provider: string,
  id: string,
): CatalogEntry | undefined {
  return entriesFor(catalog, kind, provider).find((m) => m.id === id);
}

/** Free models first, then by input price. */
export function presetEntries(
  catalog: ModelCatalog | null,
  kind: ModelKind,
  provider: string,
): CatalogEntry[] {
  const all = entriesFor(catalog, kind, provider);
  const ids = PRESET_IDS[kind][provider as ProviderId] ?? [];
  const cost = (m: CatalogEntry) => m.in_per_m ?? Number.MAX_SAFE_INTEGER;
  return ids
    .map((id) => all.find((m) => m.id === id))
    .filter((m): m is CatalogEntry => Boolean(m))
    .sort((a, b) => (a.in_per_m === 0 ? -1 : cost(a)) - (b.in_per_m === 0 ? -1 : cost(b)));
}

const num = new Intl.NumberFormat("tr-TR", { maximumFractionDigits: 4 });

export function priceLabel(m: CatalogEntry): string {
  if (m.per_hour != null) return `${num.format(m.per_hour)} $ / saat`;
  if (m.in_per_m === 0 && m.out_per_m === 0) return "Ücretsiz";
  if (m.in_per_m == null && m.out_per_m == null) return "Fiyat yok";
  const f = (v: number | null) => (v == null ? "?" : `$${num.format(v)}`);
  return `${f(m.in_per_m)} / ${f(m.out_per_m)} · 1M token`;
}

export function priceShort(m: CatalogEntry): string {
  if (m.per_hour != null) return `${num.format(m.per_hour)} $/sa`;
  if (m.in_per_m === 0 && m.out_per_m === 0) return "ücretsiz";
  if (m.in_per_m == null) return "fiyat yok";
  return `$${num.format(m.in_per_m)}`;
}

export function contextLabel(n: number): string | null {
  if (!n) return null;
  if (n >= 1_000_000) return `${num.format(Math.round(n / 100_000) / 10)}M`;
  if (n >= 1000) return `${Math.round(n / 1000)}K`;
  return String(n);
}

export const SOURCE_LABEL: Record<PricingSource, string> = {
  live: "canlı fiyat",
  static: "tarihli fiyat",
  none: "fiyat yok",
};

// OpenRouter wants `vendor/model`, Groq a bare name. A bare slug on OpenRouter
// doesn't fail as unknown, it resolves to a non-audio model and 400s.
// Mirrors `core::SttSettings::is_model_valid_for`.
export function isSttModelValidFor(provider: string, id: string): boolean {
  const model = id.trim();
  if (!model) return false;
  if (provider === "groq") return !model.includes("/");
  if (provider === "openrouter") {
    const parts = model.split("/");
    return parts.length === 2 && !!parts[0].trim() && !!parts[1].trim();
  }
  return true;
}

export function fallbackSttModel(
  catalog: ModelCatalog | null,
  provider: string,
): string | undefined {
  return presetEntries(catalog, "stt", provider)[0]?.id;
}
