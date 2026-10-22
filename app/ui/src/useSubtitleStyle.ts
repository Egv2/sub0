// Mirrored to CSS variables on the root so caption styles need no prop drilling.
import { useCallback, useEffect, useState } from "react";
import {
  DEFAULT_STYLE,
  loadStyle,
  saveStyle,
  styleVars,
  type SubtitleStyle,
} from "./subtitleStyle";

let cache: SubtitleStyle | null = null;

export function useSubtitleStyle() {
  const [style, setStyle] = useState<SubtitleStyle>(() => cache ?? loadStyle());

  const update = useCallback((next: SubtitleStyle) => {
    cache = next;
    saveStyle(next);
    setStyle(next);
  }, []);

  useEffect(() => {
    const el = document.documentElement;
    for (const [k, v] of Object.entries(styleVars(style))) {
      el.style.setProperty(k, v);
    }
  }, [style]);

  return { style, update, defaults: DEFAULT_STYLE };
}
