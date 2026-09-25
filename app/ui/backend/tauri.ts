import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Backend, Display, Entry, FileResult, HeaderDoc, KeywordInfo, Op, Opened, Pixels, Plan, Readout, Thumb } from "./types";

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
  initialPath: () => invoke<string | null>("initial_path"),
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
};
