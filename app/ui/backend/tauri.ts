import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Backend, HeaderDoc, PreviewInfo } from "./types";

export const tauriBackend: Backend = {
  header: (path) => invoke<HeaderDoc>("header", { path }),

  async preview(path, maxEdge) {
    const info = await invoke<PreviewInfo>("preview_info", { path, maxEdge });
    // Raw little-endian f32 bytes, sent as an ArrayBuffer (no JSON).
    const buf = await invoke<ArrayBuffer>("preview_pixels");
    return { info, pixels: new Float32Array(buf) };
  },

  initialPath: () => invoke<string | null>("initial_path"),

  async pickFile() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "FITS", extensions: ["fit", "fits", "fts", "fz"] }],
    });
    return typeof picked === "string" ? picked : null;
  },
};
