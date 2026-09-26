import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Backend, HeaderDiff, BatchPlan, BlinkFrame, Bytes, Distribution, Rig, FrameStats, Matching, Move, PackReport, Report, Display, ExportPlan, Exported, Entry, FileResult, HeaderDoc, KeywordInfo, Op, Opened, Pixels, Plan, Readout, Thumb, OrganizePlan } from "./types";

/** Binary payloads: [u32 width][u32 height][u32 channels] little-endian, then data. */
export function unpack(buf: ArrayBuffer): { width: number; height: number; channels: number; body: ArrayBuffer } {
  const head = new DataView(buf, 0, 12);
  return {
    width: head.getUint32(0, true),
    height: head.getUint32(4, true),
    channels: head.getUint32(8, true),
    body: buf.slice(12),
  };
}

function pixels(buf: ArrayBuffer): Pixels {
  const p = unpack(buf);
  return { width: p.width, height: p.height, channels: p.channels, data: new Uint16Array(p.body) };
}

const FITS = [{ name: "FITS", extensions: ["fit", "fits", "fts", "fz"] }];

export const tauriBackend: Backend = {
  initialPath: () => invoke<{ path: string; dir: boolean } | null>("initial_path"),
  async pickFile() {
    const p = await open({ multiple: false, filters: FITS });
    return typeof p === "string" ? p : null;
  },
  async pickFolder() {
    const p = await open({ directory: true, multiple: false });
    return typeof p === "string" ? p : null;
  },
  listFolder: (path) => invoke<Entry[]>("list_folder", { path }),
  async thumbnail(path) {
    try {
      const p = unpack(await invoke<ArrayBuffer>("thumbnail", { path }));
      return { width: p.width, height: p.height, rgba: new Uint8ClampedArray(p.body) } satisfies Thumb;
    } catch {
      return null;
    }
  },
  openFile: (path) => invoke<Opened>("open_file", { path }),
  display: (mode) => invoke<Display>("display", { mode }),
  preview: async (mode, maxEdge) => pixels(await invoke<ArrayBuffer>("preview", { mode, maxEdge })),
  region: async (mode, x, y, w, h, step) =>
    pixels(await invoke<ArrayBuffer>("region", { mode, x, y, w, h, step })),
  readout: (x, y) => invoke<Readout | null>("readout", { x, y }),
  header: (path) => invoke<HeaderDoc>("header", { path }),
  dictionary: () => invoke<KeywordInfo[]>("dictionary"),
  log: (msg) => void invoke("log", { msg }).catch(() => {}),
  planEdit: (path, ops, options) => invoke<Plan>("plan_edit", { path, ops, options }),
  applyEdits: (paths, ops, options) => invoke<FileResult[]>("apply_edits", { paths, ops, options }),
  scrubOps: (path) => invoke<Op[]>("scrub_ops", { path }),
  exportPlan: (spec, template) => invoke<ExportPlan>("export_plan", { spec, template }),
  async exportPreview(spec, maxEdge) {
    const p = unpack(await invoke<ArrayBuffer>("export_preview", { spec, maxEdge }));
    return { width: p.width, height: p.height, channels: p.channels, data: new Uint8Array(p.body) } satisfies Bytes;
  },
  exportImage: (spec, template, dir) => invoke<Exported>("export_image", { spec, template, dir }),
  packFile: (path, unpack) => invoke<PackReport>("pack_file", { path, unpack }),
  sessionReport: (path, recursive, grade, rules) => invoke<Report>("session_report", { path, recursive, grade, rules: rules ?? null }),
  saveReport: (format) => invoke<string>("save_report", { format }),
  moveRejects: (paths, dryRun) => invoke<Move[]>("move_rejects", { paths, dryRun }),
  async blinkFrame(path, maxEdge, stf) {
    const buf = await invoke<ArrayBuffer>("blink_frame", { path, maxEdge, stf: stf ?? null });
    const head = new DataView(buf, 0, 52);
    const u = (i: number) => head.getUint32(i * 4, true);
    const f = Array.from({ length: 9 }, (_, i) => head.getFloat32(16 + i * 4, true));
    return { width: u(0), height: u(1), bottomUp: u(3) === 1, stf: f, rgba: new Uint8ClampedArray(buf, 52) } satisfies BlinkFrame;
  },
  subStats: (path) => invoke<FrameStats>("sub_stats", { path }),
  diffFiles: (a, b) => invoke<HeaderDiff>("diff_files", { a, b }),
  keywordSpread: (paths) => invoke<Distribution>("keyword_spread", { paths }),
  batchPlan: (paths, ops, options) => invoke<BatchPlan>("batch_plan", { paths, ops, options }),
  batchApply: (paths, ops, options) => invoke<[string, string][]>("batch_apply", { paths, ops, options }),
  rigsList: () => invoke<Rig[]>("rigs_list"),
  rigSave: (name, from, keys) => invoke<Rig>("rig_save", { name, from, keys }),
  rigDelete: (name) => invoke<boolean>("rig_delete", { name }),
  organizePlan: (folder, by, rename) => invoke<OrganizePlan>("organize_plan", { folder, by, rename: rename ?? null }),
  organizeApply: (plan, undo) => invoke<OrganizePlan>("organize_apply", { plan, undo: undo ?? null }),
  matchCalibration: (lights, library) => invoke<Matching>("match_calibration", { lights, library }),
};
