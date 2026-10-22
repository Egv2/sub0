import { useEffect, useRef, useState } from "react";
import { IconChevronDown } from "./icons";
import { langName, useLanguages } from "../languages";
import LanguageSelect from "./LanguageSelect";
import { usePopoverSide } from "../usePopoverSide";

interface LanguageMultiSelectProps {
  selected: string[];
  onToggle: (code: string) => void;
}

export default function LanguageMultiSelect({ selected, onToggle }: LanguageMultiSelectProps) {
  // Re-render once language names load.
  useLanguages();
  const [open, setOpen] = useState(false);
  const popRef = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const side = usePopoverSide(open, btnRef);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      const t = e.target as Node;
      if (popRef.current?.contains(t) || btnRef.current?.contains(t)) return;
      setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("click", close);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("click", close);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  const names = selected.map((code) => langName(code).replace(/\s*\([^)]*\)$/, ""));
  const label =
    names.length === 0
      ? "Dil seç"
      : names.length <= 2
        ? names.join(", ")
        : `${names.slice(0, 2).join(", ")} +${names.length - 2}`;

  return (
    <div className="lm">
      <button
        ref={btnRef}
        type="button"
        className="lm-trigger"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span className={"lm-label" + (selected.length ? "" : " lm-empty")}>{label}</span>
        <IconChevronDown size={14} />
      </button>
      {!open ? null : (
        <div className="lang-pop lm-pop" data-side={side} ref={popRef}>
          <LanguageSelect
            selected={selected}
            onToggle={onToggle}
            autoFocus
            footer={
              <span className="hint">
                {selected.length === 0 ? "dil seçilmedi" : `${selected.length} dil seçili`}
              </span>
            }
          />
        </div>
      )}
    </div>
  );
}
