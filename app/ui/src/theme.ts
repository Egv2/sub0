// Stored in localStorage, not settings.toml, so it applies before the backend
// answers. :root styles are the mono theme; the rest use [data-theme].
export type ThemeId = "mono" | "dark" | "dusk" | "light" | "lemon";

export const DEFAULT_THEME: ThemeId = "mono";

export interface ThemeDef {
  id: ThemeId;
  label: string;
  hint: string;
  /** ground, surface, accent */
  swatch: string[];
}

export const THEMES: ThemeDef[] = [
  {
    id: "mono",
    label: "Koyu (gri)",
    hint: "Monokrom, gri ve siyah",
    swatch: ["#0a0a0a", "#121212", "#ededed"],
  },
  {
    id: "dusk",
    label: "Mavi (gece)",
    hint: "Lacivert zemin, pastel pembe aksan",
    swatch: ["#232946", "#2a3157", "#eebbc3"],
  },
  {
    id: "dark",
    label: "Koyu (yeşil)",
    hint: "Sıcak zemin, pastel yeşil aksan",
    swatch: ["#141412", "#1d1d1a", "#cdea9f"],
  },
  {
    id: "light",
    label: "Açık",
    hint: "Beyaz-gri, nötr ve keskin",
    swatch: ["#f7f7f7", "#ffffff", "#171717"],
  },
  {
    id: "lemon",
    label: "Açık (sarı)",
    hint: "Beyaz zemin, lacivert metin, sarı aksan",
    swatch: ["#fffffe", "#e3f6f5", "#ffd803"],
  },
];

export const THEME_STORAGE_KEY = "sub0.theme";

export function isThemeId(v: unknown): v is ThemeId {
  return (
    v === "mono" || v === "dark" || v === "dusk" || v === "light" || v === "lemon"
  );
}

export function readTheme(): ThemeId {
  try {
    const v = localStorage.getItem(THEME_STORAGE_KEY);
    return isThemeId(v) ? v : DEFAULT_THEME;
  } catch {
    // localStorage can throw (e.g. disabled storage).
    return DEFAULT_THEME;
  }
}

// Call before the first render to avoid a flash of the wrong theme.
export function applyTheme(id: ThemeId): void {
  document.documentElement.dataset.theme = id;
  try {
    localStorage.setItem(THEME_STORAGE_KEY, id);
  } catch {
    // Not persisted; still applies for this session.
  }
}
