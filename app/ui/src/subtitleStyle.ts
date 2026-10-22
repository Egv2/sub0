// Preview-only styling. SRT has no styling, so this stays in localStorage
// rather than settings.toml, where it would look like it affects the output.

export interface Swatch {
  id: string;
  label: string;
  /** "r g b", so CSS can do `rgb(var(--cap-bg-rgb) / var(--cap-alpha))`. */
  rgb: string;
}

export const INK_SWATCHES: Swatch[] = [
  { id: "white", label: "Beyaz", rgb: "244 244 240" },
  { id: "yellow", label: "Sarı", rgb: "245 215 110" },
  { id: "blue", label: "Açık mavi", rgb: "188 216 242" },
  { id: "orange", label: "Turuncu", rgb: "240 168 104" },
  { id: "grey", label: "Açık gri", rgb: "212 212 207" },
];

export const BG_SWATCHES: Swatch[] = [
  { id: "black", label: "Siyah", rgb: "0 0 0" },
  { id: "charcoal", label: "Antrasit", rgb: "22 24 26" },
  { id: "navy", label: "Lacivert", rgb: "13 27 42" },
  { id: "plum", label: "Erik moru", rgb: "27 16 32" },
  { id: "forest", label: "Koyu yeşil", rgb: "11 26 18" },
];

// Below 20% the backing is too faint to read against video.
export const MIN_OPACITY = 20;
export const MAX_OPACITY = 100;

export interface SubtitleStyle {
  ink: string;
  bg: string;
  /** 0–100. */
  opacity: number;
}

export const DEFAULT_STYLE: SubtitleStyle = { ink: "white", bg: "black", opacity: 88 };

export const STYLE_STORAGE_KEY = "sub0.subtitle-style";

export function isSwatchId(list: Swatch[], v: unknown): v is string {
  return typeof v === "string" && list.some((s) => s.id === v);
}

export function inkRgb(id: string): string {
  return INK_SWATCHES.find((s) => s.id === id)?.rgb ?? INK_SWATCHES[0].rgb;
}

export function bgRgb(id: string): string {
  return BG_SWATCHES.find((s) => s.id === id)?.rgb ?? BG_SWATCHES[0].rgb;
}

/** Falls back per field on missing or out-of-range values. */
export function loadStyle(): SubtitleStyle {
  try {
    const raw = localStorage.getItem(STYLE_STORAGE_KEY);
    if (!raw) return DEFAULT_STYLE;
    const parsed = JSON.parse(raw) as Partial<SubtitleStyle>;
    const opacity = Number(parsed.opacity);
    return {
      ink: isSwatchId(INK_SWATCHES, parsed.ink) ? parsed.ink : DEFAULT_STYLE.ink,
      bg: isSwatchId(BG_SWATCHES, parsed.bg) ? parsed.bg : DEFAULT_STYLE.bg,
      opacity:
        Number.isFinite(opacity) && opacity >= MIN_OPACITY && opacity <= MAX_OPACITY
          ? Math.round(opacity)
          : DEFAULT_STYLE.opacity,
    };
  } catch {
    return DEFAULT_STYLE;
  }
}

export function saveStyle(style: SubtitleStyle): void {
  try {
    localStorage.setItem(STYLE_STORAGE_KEY, JSON.stringify(style));
  } catch {
    // Storage can be unavailable (private mode); not worth surfacing.
  }
}

export function styleVars(style: SubtitleStyle): Record<string, string> {
  return {
    "--cap-ink-rgb": inkRgb(style.ink),
    "--cap-bg-rgb": bgRgb(style.bg),
    "--cap-alpha": String(style.opacity / 100),
  };
}
