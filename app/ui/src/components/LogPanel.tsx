// No Card wrapper: this renders inside the library card.
import { useEffect, useRef, useState } from "react";
import type { LogLine } from "../types";
import { fmtClock } from "../utils";
import { CardHead, IconBtn } from "./ui";
import { IconActivity, IconChevronDown, IconClock } from "./icons";

// Within this many px of the bottom we follow new lines; otherwise leave the scroll alone.
const BOTTOM_SLACK = 24;

// Word reveal fits in roughly 420ms whatever the line length, max 28ms per word.
const WORD_WINDOW = 420;
const WORD_GAP_MAX = 28;

export default function LogPanel({ lines }: { lines: LogLine[] }) {
  const [collapsed, setCollapsed] = useState(false);
  const [missed, setMissed] = useState(0);
  const [away, setAway] = useState(false);
  const [streamId, setStreamId] = useState<number | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  // A ref so the arrival effect reads the current value; `away` only drives the button.
  const atBottomRef = useRef(true);
  const prevLastIdRef = useRef<number | null>(null);

  const lastId = lines.length > 0 ? lines[lines.length - 1].id : null;

  function onScroll() {
    const el = scrollRef.current;
    if (!el) return;
    const atB = el.scrollHeight - el.scrollTop - el.clientHeight <= BOTTOM_SLACK;
    atBottomRef.current = atB;
    setAway(!atB);
    if (atB) setMissed(0);
  }

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return; // collapsed; expanding jumps to the latest anyway
    if (atBottomRef.current) el.scrollTop = el.scrollHeight;
    else setMissed((m) => m + 1);
  }, [lastId]);

  // Only animate a genuinely new line, not on remount or expand.
  useEffect(() => {
    if (lastId !== null && lastId !== prevLastIdRef.current) setStreamId(lastId);
    prevLastIdRef.current = lastId;
  }, [lastId]);

  useEffect(() => {
    if (collapsed) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
    atBottomRef.current = true;
    setStreamId(null);
    setAway(false);
    setMissed(0);
  }, [collapsed]);

  function jumpToLatest() {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
    atBottomRef.current = true;
    setAway(false);
    setMissed(0);
  }

  return (
    <div className={"log-nested" + (collapsed ? " collapsed" : "")}>
      <CardHead
        icon={<IconActivity size={16} />}
        title="Olay akışı"
        right={
          <>
            {lines.length > 0 && <span className="ev-count">{lines.length} olay</span>}
            <IconBtn
              className="sm chev-btn"
              title={collapsed ? "Genişlet" : "Katla"}
              onClick={() => setCollapsed((c) => !c)}
            >
              <IconChevronDown size={15} className="chev" />
            </IconBtn>
          </>
        }
      />
      {!collapsed &&
        (lines.length === 0 ? (
          <div className="events-empty">
            <span className="e-icon">
              <IconClock size={18} />
            </span>
            <div className="e-t">Henüz bir olay yok</div>
            <div className="e-s">İşler başladığında burada görünecek</div>
          </div>
        ) : (
          <div className="ev-box">
            <div ref={scrollRef} className="ev-stream" onScroll={onScroll}>
              {lines.map((l) => (
                <div
                  key={l.id}
                  className={"ev-row k-" + l.kind + (l.id === streamId ? " new" : "")}
                >
                  <span className="ev-time">{fmtClock(l.at)}</span>
                  <span className="ev-dot" aria-hidden="true" />
                  {l.jobId === null ? (
                    <span className="ev-chip off" aria-hidden="true" />
                  ) : (
                    <span className="ev-chip" title={"iş " + l.jobId}>
                      #{l.jobId}
                    </span>
                  )}
                  <span className="ev-text">
                    {l.id === streamId ? <StreamWords text={l.text} /> : l.text}
                  </span>
                </div>
              ))}
            </div>
            {away && (
              <button className="ev-jump" aria-live="polite" onClick={jumpToLatest}>
                {missed > 0 ? `${missed} yeni olay` : "En sona git"}
                <IconChevronDown size={12} />
              </button>
            )}
          </div>
        ))}
    </div>
  );
}

// Animation lives in global.css; `both` fill keeps words hidden until their delay.
function StreamWords({ text }: { text: string }) {
  const words = text.split(" ");
  const gap = Math.min(WORD_GAP_MAX, WORD_WINDOW / words.length);
  return (
    <>
      {words.map((w, i) => (
        <span key={i} className="ev-w" style={{ animationDelay: `${Math.round(i * gap)}ms` }}>
          {w + (i + 1 < words.length ? " " : "")}
        </span>
      ))}
    </>
  );
}
