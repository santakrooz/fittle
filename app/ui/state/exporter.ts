// Export dialog state. The choices persist per viewer (localStorage) so the
// dialog opens the way it was last used; nothing else depends on them.
import type { ExportFormat, ExportSpec, ExportStretch } from "../backend/types";
import { createStore } from "./createStore";
import { shaderStretch } from "./stretch";
import { app } from "./app";

export type FormatKind = ExportFormat["kind"];
export type StretchChoice = "none" | "view" | "auto";

export type ExportChoices = {
  format: FormatKind;
  pngBits: 8 | 16;
  tiffBits: 8 | 16 | 32;
  quality: number;
  stretch: StretchChoice;
  longEdge: number | null;
  template: string;
  metadata: boolean;
  private: boolean;
  debayer: boolean;
  /** Add the share-card caption strip. */
  card: boolean;
  /** Output folder; null = next to the source. */
  dir: string | null;
};

const DEFAULTS: ExportChoices = {
  format: "png",
  pngBits: 16,
  tiffBits: 16,
  quality: 90,
  stretch: "view",
  longEdge: null,
  template: "{object}_{integration}_{date}",
  metadata: true,
  private: true,
  debayer: true,
  card: false,
  dir: null,
};

const KEY = "fittle.export";

function load(): ExportChoices {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Partial<ExportChoices>;
    return { ...DEFAULTS, ...saved, dir: null };
  } catch {
    return DEFAULTS;
  }
}

export const exporter = createStore<{ open: boolean; choices: ExportChoices }>({ open: false, choices: load() });

export function setChoices(patch: Partial<ExportChoices>) {
  exporter.set((s) => ({ choices: { ...s.choices, ...patch } }));
  try {
    const { dir: _dir, ...keep } = exporter.get().choices;
    localStorage.setItem(KEY, JSON.stringify(keep));
  } catch {
    /* storage unavailable: choices last for this session only */
  }
}

/** Open the dialog; `withView` preselects the viewer's current stretch. */
export function openExport(withView = false) {
  if (!app.get().opened?.image) return;
  if (withView) setChoices({ stretch: "view" });
  exporter.set({ open: true });
}

export const closeExport = () => exporter.set({ open: false });

function format(c: ExportChoices): ExportFormat {
  switch (c.format) {
    case "png":
      return { kind: "png", bits: c.pngBits };
    case "jpeg":
      return { kind: "jpeg", quality: c.quality };
    case "avif":
      return { kind: "avif", quality: c.quality };
    case "tiff":
      return { kind: "tiff", bits: c.tiffBits };
    case "webp":
      return { kind: "webp" };
    default:
      return { kind: "fits" };
  }
}

/** The viewer's stretch as exact parameters, so the file matches the screen. */
function viewStretch(): ExportStretch {
  const s = app.get();
  if (!s.display) return { kind: "auto", linked: s.stretch.linked };
  const sh = shaderStretch(s.display, s.stretch);
  return {
    kind: "custom",
    stf: [0, 1, 2].map((i) => ({ shadows: sh.shadows[i], midtones: sh.midtones[i], highlights: sh.highlights[i] })),
    asinh: sh.asinh,
  };
}

export function specOf(c: ExportChoices): ExportSpec {
  const stretch: ExportStretch =
    c.stretch === "none" ? { kind: "none" } : c.stretch === "auto" ? { kind: "auto", linked: app.get().stretch.linked } : viewStretch();
  return {
    format: format(c),
    stretch,
    debayer: c.debayer,
    crop: null,
    rotate: 0,
    flip_horizontal: false,
    flip_vertical: false,
    bin: null,
    bin_mode: "average",
    long_edge: c.longEdge,
    metadata: c.metadata,
    private: c.private,
    card: c.card && c.format !== "fits",
  };
}
