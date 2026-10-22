import type { ReactNode } from "react";

export function Card({ className = "", children }: { className?: string; children: ReactNode }) {
  return <section className={"card " + className}>{children}</section>;
}

export function CardHead({
  icon,
  title,
  right,
}: {
  icon: ReactNode;
  title: string;
  right?: ReactNode;
}) {
  return (
    <header className="card-head">
      <span className="card-head-icon">{icon}</span>
      <h2 className="card-title">{title}</h2>
      {right !== undefined && <div className="card-head-right">{right}</div>}
    </header>
  );
}

export function Chip({ label, value, tone }: { label: string; value: string; tone?: "ok" }) {
  return (
    <span className={"chip" + (tone === "ok" ? " ok" : "")}>
      <span className="chip-label">{label}</span>
      <span className="chip-value">{value}</span>
    </span>
  );
}

export function Toggle({
  checked,
  onChange,
  title,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  title?: string;
}) {
  return (
    <label className="switch" title={title}>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span />
    </label>
  );
}

export function IconBtn({
  onClick,
  title,
  className = "",
  children,
}: {
  onClick?: () => void;
  title: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <button className={"icon-btn " + className} title={title} aria-label={title} onClick={onClick}>
      {children}
    </button>
  );
}
