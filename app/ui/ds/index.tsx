// Fittle design-system components (docs/design-system). Typed ports of the
// style guide's reference bundle; styles in styles/components.css.
import type { ReactNode } from "react";

const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(" ");

const ICONS = {
  sub: "M3 7h18v10H3zM8 7l1.5-2h5L16 7M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
  stack: "M12 3l9 5-9 5-9-5 9-5zM3 13l9 5 9-5",
  cal: "M4 4h16v16H4zM4 12h16M12 4v16",
  zoomIn: "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM21 21l-5-5M11 8v6M8 11h6",
  zoomOut: "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM21 21l-5-5M8 11h6",
  search: "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM21 21l-5-5",
  folder: "M3 6h6l2 2h10v11H3z",
  file: "M6 3h8l5 5v13H6zM14 3v5h5",
  export: "M12 4v11M7 10l5 5 5-5M5 20h14",
  header: "M4 6h16M4 12h16M4 18h10",
  histogram: "M4 20V10M9 20V4M14 20v-8M19 20v-5",
  eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
  clip: "M4 4l16 16M20 4 4 20",
  lock: "M6 11h12v9H6zM8 11V8a4 4 0 0 1 8 0v3",
} as const;
export type IconName = keyof typeof ICONS;

export function Icon({ name }: { name: IconName }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d={ICONS[name]} />
    </svg>
  );
}

export function Button(p: {
  variant?: "primary" | "secondary" | "ghost";
  icon?: IconName;
  disabled?: boolean;
  title?: string;
  onClick?: () => void;
  children: ReactNode;
}) {
  return (
    <button type="button" className={cx("ft-btn", p.variant ?? "secondary")} disabled={p.disabled} title={p.title} onClick={p.onClick}>
      {p.icon && <Icon name={p.icon} />}
      {p.children}
    </button>
  );
}

export function Chip(p: { on?: boolean; onClick?: () => void; children: ReactNode }) {
  return (
    <button type="button" className="ft-chip" aria-pressed={!!p.on} onClick={p.onClick}>
      {p.children}
    </button>
  );
}

export function Segmented<T extends string>(p: { options: { value: T; label: string }[]; value: T; onChange: (v: T) => void; label: string }) {
  return (
    <div className="ft-seg" role="radiogroup" aria-label={p.label}>
      {p.options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={o.value === p.value} onClick={() => p.onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Tabs<T extends string>(p: { tabs: { value: T; label: string }[]; value: T; onChange: (v: T) => void }) {
  return (
    <div className="ft-tabs" role="tablist">
      {p.tabs.map((t) => (
        <button key={t.value} type="button" role="tab" className="ft-tab" aria-selected={t.value === p.value} onClick={() => p.onChange(t.value)}>
          {t.label}
        </button>
      ))}
    </div>
  );
}

export function Toolbar({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="ft-toolbar" role="toolbar" aria-label={label}>
      {children}
    </div>
  );
}

export function ToolButton(p: { on?: boolean; icon?: IconName; label?: string; disabled?: boolean; onClick?: () => void; children?: ReactNode }) {
  return (
    <button
      type="button"
      className="ft-tool"
      aria-pressed={!!p.on}
      aria-label={p.label}
      title={p.label}
      disabled={p.disabled}
      onClick={p.onClick}
    >
      {p.icon && <Icon name={p.icon} />}
      {p.children}
    </button>
  );
}

export const Separator = () => <span className="ft-sep" aria-hidden="true" />;

export function Card(p: { title?: string; badge?: ReactNode; children?: ReactNode; className?: string }) {
  return (
    <section className={cx("ft-card", p.className)} aria-label={p.title}>
      {(p.title || p.badge) && (
        <div className="ft-card-head">
          <span className="ft-card-title">{p.title}</span>
          {p.badge}
        </div>
      )}
      {p.children}
    </section>
  );
}

export type Tone = "accent" | "orange" | "rose" | "teal" | "ok" | "warn" | "bad";
export function Badge({ tone = "accent", children, title }: { tone?: Tone; children: ReactNode; title?: string }) {
  return (
    <span className={cx("ft-badge", tone)} title={title}>
      {children}
    </span>
  );
}

export function Fields({ children }: { children: ReactNode }) {
  return <div className="ft-fields">{children}</div>;
}

export function Field(p: {
  label: string;
  value: ReactNode;
  unit?: ReactNode;
  /** Computed, not read from the header. */
  derived?: boolean;
  /** Filled from the scope profile because the header lacked it. */
  fallback?: boolean;
  title?: string;
  wide?: boolean;
}) {
  return (
    <div title={p.title} style={p.wide ? { gridColumn: "1 / -1" } : undefined}>
      <div className="ft-label">
        {p.label}
        {p.derived && <span className="ft-derived">DERIVED</span>}
        {p.fallback && <span className="ft-default">DEFAULT</span>}
      </div>
      <div className="ft-value">
        {p.value}
        {p.unit != null && <small> {p.unit}</small>}
      </div>
    </div>
  );
}

export function StatTile(p: { value: ReactNode; unit?: string; label: string }) {
  return (
    <div className="ft-tile">
      <div className="ft-tile-n">
        {p.value}
        {p.unit && <small> {p.unit}</small>}
      </div>
      <span className="ft-label">{p.label}</span>
    </div>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="ft-kbd">{children}</kbd>;
}

export function EvidenceChip({ text }: { text: string }) {
  const i = text.indexOf("=");
  return (
    <span className="ft-ev">
      {i > 0 ? (
        <>
          {text.slice(0, i + 1)}
          <b>{text.slice(i + 1)}</b>
        </>
      ) : (
        text
      )}
    </span>
  );
}
