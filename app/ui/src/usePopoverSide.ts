// Opens upward when there isn't room below the trigger.
import { useLayoutEffect, useState, type CSSProperties, type RefObject } from "react";

export type Side = "top" | "bottom";

const WANT = 360;

export function usePopoverSide(open: boolean, ref: RefObject<HTMLElement>): Side {
  const [side, setSide] = useState<Side>("bottom");
  useLayoutEffect(() => {
    if (!open) return;
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    const below = window.innerHeight - r.bottom;
    const above = r.top;
    setSide(below >= WANT || below >= above ? "bottom" : "top");
  }, [open, ref]);
  return side;
}

// For popovers portaled to <body>. The trigger lives inside overflow: auto
// containers (.view, .pv-left) that would clip it, and z-index can't escape that.
export interface PopoverPosition {
  side: Side;
  style: CSSProperties;
}

export function usePopoverPosition(
  open: boolean,
  ref: RefObject<HTMLElement | null>,
): PopoverPosition | null {
  const [pos, setPos] = useState<PopoverPosition | null>(null);
  useLayoutEffect(() => {
    if (!open) return;
    const el = ref.current;
    if (!el) return;
    const place = () => {
      const r = el.getBoundingClientRect();
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      // Right-aligned to the trigger, clamped to the viewport.
      const width = Math.min(460, vw - 32);
      const left = Math.min(Math.max(r.right - width, 16), Math.max(vw - width - 16, 16));
      const below = vh - r.bottom;
      const above = r.top;
      if (below >= WANT || below >= above) {
        setPos({
          side: "bottom",
          style: { left, top: r.bottom + 6, width, maxHeight: Math.max(Math.min(420, below - 22), 120) },
        });
      } else {
        setPos({
          side: "top",
          style: { left, bottom: vh - r.top + 6, width, maxHeight: Math.max(Math.min(420, above - 22), 120) },
        });
      }
    };
    place();
    // Capture phase, because scroll events on ancestors don't bubble.
    window.addEventListener("scroll", place, true);
    window.addEventListener("resize", place);
    return () => {
      window.removeEventListener("scroll", place, true);
      window.removeEventListener("resize", place);
    };
  }, [open, ref]);
  return open ? pos : null;
}
