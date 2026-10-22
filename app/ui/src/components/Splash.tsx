import { useEffect, useRef, useState } from "react";
import jelly from "../assets/trans-jelly.webm";

const MIN_VISIBLE_MS = 2000;
const MIN_VISIBLE_REDUCED_MS = 700;

const FIRST_FRAME_TIMEOUT_MS = 800;
const EXIT_MS = 480;

interface SplashProps {
  ready: boolean;

  onVisible: () => void;
}

export default function Splash({ ready, onVisible }: SplashProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const visibleAt = useRef<number | null>(null);
  const onVisibleRef = useRef(onVisible);
  onVisibleRef.current = onVisible;
  const [shown, setShown] = useState(false);
  const [closing, setClosing] = useState(false);
  const [mounted, setMounted] = useState(true);
  const reduced =
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  useEffect(() => {
    // Reveal the native window right away: the splash is the first paint and
    // WebKit pauses rAF and media decoding in hidden windows, so waiting for a
    // video frame here would stall. The minimum display time starts once the
    // first frame is actually up.
    onVisibleRef.current();
    const markVisible = () => {
      if (visibleAt.current != null) return;
      visibleAt.current = performance.now();
      setShown(true);
    };
    const v = videoRef.current;
    if (v) {
      if (reduced) v.pause();
      if (v.readyState >= 2) markVisible();
      else v.addEventListener("loadeddata", markVisible, { once: true });
    }
    const fallback = setTimeout(markVisible, FIRST_FRAME_TIMEOUT_MS);
    return () => {
      clearTimeout(fallback);
      v?.removeEventListener("loadeddata", markVisible);
    };
  }, [reduced]);

  useEffect(() => {
    if (!ready || !shown) return;
    const min = reduced ? MIN_VISIBLE_REDUCED_MS : MIN_VISIBLE_MS;
    const elapsed =
      performance.now() - (visibleAt.current ?? performance.now());
    const t = setTimeout(() => setClosing(true), Math.max(0, min - elapsed));
    return () => clearTimeout(t);
  }, [ready, shown, reduced]);

  useEffect(() => {
    if (!closing) return;
    const t = setTimeout(
      () => {
        videoRef.current?.pause();
        setMounted(false);
      },
      reduced ? 0 : EXIT_MS,
    );
    return () => clearTimeout(t);
  }, [closing, reduced]);

  if (!mounted) return null;
  return (
    <div
      className={"splash" + (closing ? " closing" : "")}
      role="status"
      aria-live="polite"
      aria-label="Sub0 açılıyor"
      aria-busy={!closing}
    >
      <video
        ref={videoRef}
        src={jelly}
        autoPlay={!reduced}
        muted
        loop
        playsInline
        preload="auto"
        aria-hidden="true"
      />
    </div>
  );
}
