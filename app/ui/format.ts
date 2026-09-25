// Human formatting, matching the CLI (crates/fittle-cli/src/fmt.rs).

export const num = (v: number, places = 2) => {
  const s = v.toFixed(places);
  return s.includes(".") ? s.replace(/\.?0+$/, "") : s;
};

export const group = (v: number) => Math.round(v).toLocaleString("en-US");

export function duration(s: number): string {
  const t = Math.round(s);
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  if (h > 0) return `${h} h ${String(m).padStart(2, "0")} m`;
  if (m > 0) return `${m} m`;
  return `${t} s`;
}

/** `20h 57m 10.8s` */
export function raHms(deg: number): string {
  let h = deg / 15;
  let hh = Math.floor(h);
  let mm = Math.floor((h - hh) * 60);
  let ss = Math.round(((h - hh) * 60 - mm) * 600) / 10;
  if (ss >= 60) (ss -= 60), mm++;
  if (mm >= 60) (mm -= 60), (hh = (hh + 1) % 24);
  h = hh;
  return `${String(h).padStart(2, "0")}h ${String(mm).padStart(2, "0")}m ${ss.toFixed(1).padStart(4, "0")}s`;
}

/** `+31° 14′ 07″` */
export function decDms(deg: number): string {
  const sign = deg < 0 ? "−" : "+";
  const t = Math.round(Math.abs(deg) * 3600);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${sign}${p(Math.floor(t / 3600))}° ${p(Math.floor(t / 60) % 60)}′ ${p(t % 60)}″`;
}

export const minus = (s: string) => s.replace(/^-/, "−");

export function bitDepth(bitpix?: number): string {
  switch (bitpix) {
    case 8:
      return "8-bit";
    case 16:
      return "16-bit";
    case 32:
      return "32-bit int";
    case -32:
      return "32-bit float";
    case -64:
      return "64-bit float";
    default:
      return bitpix ? `BITPIX ${bitpix}` : "";
  }
}

export const baseName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

/** Keep the distinctive tail of long capture names: `…_20260924-213412.fit`. */
export function shortName(name: string, max = 22): string {
  if (name.length <= max) return name;
  return "…" + name.slice(name.length - max + 1);
}

/** ADU-ish values: integers grouped, small floats with 4 decimals. */
export function adu(v: number): string {
  if (!Number.isFinite(v)) return "—";
  if (Math.abs(v) >= 100) return group(v);
  return num(v, 4);
}
