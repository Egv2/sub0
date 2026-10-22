import type { ComponentType } from "react";
import { IconCaptions, IconGear, IconHome, type IconProps } from "./icons";

export type View = "studio" | "preview" | "settings";

const ITEMS: { v: View; label: string; icon: ComponentType<IconProps> }[] = [
  { v: "studio", label: "Stüdyo", icon: IconHome },
  { v: "preview", label: "Önizleme", icon: IconCaptions },
  { v: "settings", label: "Ayarlar", icon: IconGear },
];

interface NavProps {
  view: View;
  onNavigate: (v: View) => void;
  queueState: "idle" | "running" | "failed";
}

export default function Nav({ view, onNavigate, queueState }: NavProps) {
  const statusLabel = queueState === "running" ? "Çalışıyor" : queueState === "failed" ? "Hata" : "Hazır";
  return (
    <aside id="nav">
      <div className="nav-logo" title="Sub0">S</div>
      <nav id="nav-buttons">
        {ITEMS.map(({ v, label, icon: Icon }) => (
          <button
            key={v}
            className={view === v ? "active" : ""}
            title={label}
            aria-label={label}
            onClick={() => onNavigate(v)}
          >
            <Icon size={20} />
          </button>
        ))}
      </nav>
      <div className="nav-foot">
        <span
          className={"dot" + (queueState === "running" ? " running" : queueState === "failed" ? " failed" : "")}
          title={statusLabel}
        />
        <span className="ver">v0.2.0</span>
      </div>
    </aside>
  );
}
