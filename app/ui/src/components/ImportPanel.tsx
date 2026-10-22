import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { events, pickFiles } from "../api";
import type { JobRow } from "../types";
import { jobTitle } from "../utils";
import { Card, CardHead, IconBtn } from "./ui";
import Dropzone from "./Dropzone";
import { IconClock, IconFolderPlus, IconLink, IconUpload } from "./icons";

const VIDEO_FILTERS = [
  {
    name: "Video",
    extensions: ["mp4", "mkv", "mov", "webm", "avi", "m4v", "mp3", "m4a", "wav", "flac"],
  },
];

interface ImportPanelProps {
  waitingJobs: JobRow[];
  onEnqueue: (inputs: string[]) => void;
  onRemove: (jobId: number) => void;
}

export default function ImportPanel({ waitingJobs, onEnqueue, onRemove }: ImportPanelProps) {
  const [drag, setDrag] = useState(false);
  const [url, setUrl] = useState("");
  const enqueueRef = useRef(onEnqueue);
  enqueueRef.current = onEnqueue;

  // Tauri drag events are window-wide: a drop anywhere enqueues.
  useEffect(() => {
    return events.listenDrag((paths, phase) => {
      if (phase === "enter" || phase === "over") {
        setDrag(true);
      } else {
        setDrag(false);
        if (paths && paths.length) enqueueRef.current(paths);
      }
    });
  }, []);

  const pick = async () => {
    try {
      const paths = await pickFiles({ multiple: true, filters: VIDEO_FILTERS });
      if (paths && Array.isArray(paths) && paths.length) onEnqueue(paths);
    } catch {
      // dialog cancelled
    }
  };

  const addUrl = () => {
    const v = url.trim();
    if (!v) return;
    setUrl("");
    onEnqueue([v]);
  };

  const hasItems = waitingJobs.length > 0;

  return (
    <Card className={"bento-import" + (drag && hasItems ? " dragging" : "")}>
      <CardHead icon={<IconUpload size={16} />} title="Video aktar" />
      {!hasItems ? (
        <Dropzone
          dragging={drag}
          onPick={() => void pick()}
          label="Video dosyalarını buraya sürükle"
          hint="MP4, MKV, MOV, WEBM veya ses dosyası"
        />
      ) : (
        <div className="import-list">
          {waitingJobs.map((j) => {
            const title = jobTitle(j);
            return (
              <div className="import-row" key={j.id}>
                <span className="i-icon">
                  <IconClock size={18} />
                </span>
                <span className="import-main">
                  <span className="import-title" title={j.source_ref}>
                    {title}
                  </span>
                  <div className="import-meta">
                    batch {j.batch_id}
                    <span className="sep">·</span>
                    <span>sırada</span>
                  </div>
                </span>
                <button
                  className="row-x"
                  title="Kaldır"
                  aria-label={`${title} işini kaldır`}
                  onClick={() => onRemove(j.id)}
                >
                  <X size={14} strokeWidth={2} />
                </button>
              </div>
            );
          })}
        </div>
      )}
      <div className="url-row">
        <div className="url-field">
          <IconLink size={16} />
          <input
            type="text"
            spellCheck={false}
            placeholder="Video veya playlist bağlantısı"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addUrl()}
          />
        </div>
        {hasItems && (
          <IconBtn title="Dosyadan ekle" onClick={() => void pick()}>
            <IconFolderPlus size={17} />
          </IconBtn>
        )}
        <button className="secondary" onClick={addUrl}>
          Ekle
        </button>
      </div>
    </Card>
  );
}
