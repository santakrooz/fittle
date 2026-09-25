// Display stretch maths. Must agree with fittle-image `stretch` and the shader.
import type { Display, Stf } from "../backend/types";

export type StretchKind = "auto" | "linear" | "asinh" | "manual";
export type Channel = "rgb" | "r" | "g" | "b";

export type Stretch = {
  kind: StretchKind;
  /** Same curve for all channels (from luminance) vs one per channel. */
  linked: boolean;
  /** Per-channel values used when kind = manual. */
  manual: Stf[];
  clipping: boolean;
  channel: Channel;
};

export const DEFAULT_STRETCH: Stretch = { kind: "auto", linked: false, manual: [], clipping: false, channel: "rgb" };
export const IDENTITY: Stf = { shadows: 0, midtones: 0.5, highlights: 1 };

/** Midtones transfer function. */
export function mtf(m: number, x: number): number {
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  if (m === 0.5) return x;
  return ((m - 1) * x) / ((2 * m - 1) * x - m);
}

export function applyStf(s: Stf, v: number): number {
  const x = Math.min(1, Math.max(0, (v - s.shadows) / (s.highlights - s.shadows)));
  return mtf(s.midtones, x);
}

/** Auto-STF per channel (three entries, repeated for mono). */
export function autoStf(d: Display, linked: boolean): Stf[] {
  const per = linked ? [d.stf_linked] : d.stf;
  return [0, 1, 2].map((i) => per[Math.min(i, per.length - 1)]);
}

/** asinh strength so the background (median) lands at `target` after the black point. */
export function asinhStrength(background: number, target = 0.25): number {
  if (background <= 0 || background >= target) return 1;
  let lo = 1e-3;
  let hi = 1e7;
  for (let i = 0; i < 80; i++) {
    const k = Math.sqrt(lo * hi);
    const y = Math.asinh(k * background) / Math.asinh(k);
    if (y < target) lo = k;
    else hi = k;
  }
  return Math.sqrt(lo * hi);
}

export type ShaderStretch = {
  shadows: [number, number, number];
  midtones: [number, number, number];
  highlights: [number, number, number];
  /** 0 = MTF curve, >0 = asinh with this strength. */
  asinh: number;
};

/** What the shader needs for the current display and stretch. */
export function shaderStretch(d: Display, s: Stretch): ShaderStretch {
  const pack = (stf: Stf[], asinh = 0): ShaderStretch => ({
    shadows: [stf[0].shadows, stf[1].shadows, stf[2].shadows],
    midtones: [stf[0].midtones, stf[1].midtones, stf[2].midtones],
    highlights: [stf[0].highlights, stf[1].highlights, stf[2].highlights],
    asinh,
  });
  switch (s.kind) {
    case "linear":
      return pack([IDENTITY, IDENTITY, IDENTITY]);
    case "manual":
      return pack([0, 1, 2].map((i) => s.manual[Math.min(i, s.manual.length - 1)] ?? IDENTITY));
    case "asinh": {
      const auto = autoStf(d, true);
      const black = auto[0].shadows;
      const median = d.stats.reduce((a, c) => a + c.median, 0) / d.stats.length;
      const k = asinhStrength((median - black) / (1 - black));
      return pack(
        auto.map(() => ({ shadows: black, midtones: 0.5, highlights: 1 })),
        k,
      );
    }
    default:
      return pack(autoStf(d, s.linked));
  }
}
