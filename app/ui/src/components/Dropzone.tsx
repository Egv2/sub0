/* Dropzone: a flat tonal surface and a drifting icon (motion + lucide); file
 * selection is wired to the Tauri dialog and to tauri://drag events, because
 * the HTML5 File API does not carry OS drags inside Tauri. */
import { Upload } from "lucide-react";
import { motion, useReducedMotion } from "motion/react";

interface DropzoneProps {
  /** An OS drag hovering over the card (tauri://drag-*). */
  dragging: boolean;
  /** Click, Enter or Space opens the Tauri file dialog. */
  onPick: () => void;
  label: string;
  hint?: string;
}

export default function Dropzone({ dragging, onPick, label, hint }: DropzoneProps) {
  const reduceMotion = useReducedMotion() ?? false;

  return (
    <div className="halo-dz">
      <div
        className="halo-dz-label"
        role="button"
        tabIndex={0}
        aria-label={label}
        onClick={onPick}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onPick();
          }
        }}
      >
        <div className="halo-dz-frame">
          <div
            className={"halo-dz-surface" + (dragging ? " drag" : "")}
          >
            <motion.span
              className="halo-dz-icon"
              animate={reduceMotion || dragging ? { y: 0 } : { y: [0, -3, 0] }}
              transition={{ duration: 4.5, ease: "easeInOut", repeat: Number.POSITIVE_INFINITY }}
            >
              <Upload size={20} strokeWidth={2} />
            </motion.span>
            <span className="halo-dz-text">{label}</span>
            {hint ? <span className="halo-dz-hint">{hint}</span> : null}
          </div>
        </div>
      </div>
    </div>
  );
}
