import type { ReactNode } from "react";

type IconName = "sub" | "stack" | "zoomIn" | "zoomOut" | "star" | "export";

/** Pill button. One `primary` per region. */
export function Button(props: { variant?: "primary" | "secondary" | "ghost"; icon?: IconName; disabled?: boolean; onClick?: () => void; children: ReactNode }): JSX.Element;
/** Filter pill; `on` marks the active filter. */
export function Chip(props: { on?: boolean; onClick?: () => void; children: ReactNode }): JSX.Element;
/** Two to four mutually exclusive options. */
export function Segmented(props: { options: string[]; value?: string; onChange?: (v: string) => void; label?: string }): JSX.Element;
/** Pill tabs for the inspector. */
export function Tabs(props: { tabs: string[]; value?: string; onChange?: (v: string) => void }): JSX.Element;
/** Floating pill toolbar over the stage. */
export function Toolbar(props: { label: string; children: ReactNode }): JSX.Element;
export namespace Toolbar {
  function Button(props: { on?: boolean; icon?: IconName; label?: string; onClick?: () => void; children?: ReactNode }): JSX.Element;
  function Separator(): JSX.Element;
}
/** Bordered card with an optional title row and badge. */
export function Card(props: { title?: string; badge?: ReactNode; children?: ReactNode }): JSX.Element;
/** Tinted status or category badge. */
export function Badge(props: { tone?: "accent" | "orange" | "rose" | "teal" | "ok" | "warn" | "bad"; children: ReactNode }): JSX.Element;
/** Uppercase label over a value; `derived` adds the teal DERIVED tag. */
export function Field(props: { label: string; value: ReactNode; unit?: string; derived?: boolean }): JSX.Element;
/** Two-column grid for Field. */
export function Fields(props: { children: ReactNode }): JSX.Element;
/** KPI tile: big number and label. */
export function StatTile(props: { value: ReactNode; unit?: string; label: string }): JSX.Element;
/** Verdict with confidence (0–100) and evidence chips (`KEY=value`). */
export function VerdictCard(props: { kind: "sub" | "stack"; label: string; detail?: string; confidence: number; evidence?: string[] }): JSX.Element;
/** Keyboard hint. */
export function Kbd(props: { children: ReactNode }): JSX.Element;
/** File rail row with thumbnail, name and status line. */
export function FileRow(props: { name: string; detail: string; status?: "ok" | "warn" | "rejected"; selected?: boolean; onClick?: () => void }): JSX.Element;
export function Icon(props: { name: IconName }): JSX.Element;
