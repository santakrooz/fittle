import type { Stf } from "../backend/types";

/** Midtones transfer function; identical to fittle-image `stretch::mtf`. */
export function mtf(m: number, x: number): number {
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  if (m === 0.5) return x;
  return ((m - 1) * x) / ((2 * m - 1) * x - m);
}

export function applyStf(stf: Stf, v: number): number {
  const x = Math.min(1, Math.max(0, (v - stf.shadows) / (stf.highlights - stf.shadows)));
  return mtf(stf.midtones, x);
}

export type StretchMode = "auto" | "linear";

export const LINEAR: Stf = { shadows: 0, midtones: 0.5, highlights: 1 };
