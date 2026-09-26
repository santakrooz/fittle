// `?demo`: the app in a plain browser from fixtures written by
// `cargo run -p fittle-image --example ui_fixtures -- <folders…>` (served by the
// dev server from app/demo-fixtures). Regions and readouts come from the
// preview, so they are approximate; RA/Dec is not available.
import { unpack } from "./tauri";
import type { Backend, Display, Entry, HeaderDoc, KeywordInfo, Mode, Opened, Pixels, Plan, Report, Matching, Distribution, HeaderDiff } from "./types";

export const isDemo = () => new URLSearchParams(location.search).has("demo");

const json = async <T>(url: string): Promise<T> => {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: ${r.status}`);
  return r.json() as Promise<T>;
};
const bin = async (url: string) => {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: ${r.status}`);
  return r.arrayBuffer();
};

const idOf = (path: string) => path.replace(/^demo:/, "");

export function demoBackend(): Backend {
  let current = "";
  let opened: Opened | null = null;
  const previews = new Map<string, Pixels>();
  const displays = new Map<string, Display>();

  const load = async (mode: Mode) => {
    const key = `${current}/${mode}`;
    if (!previews.has(key)) {
      const p = unpack(await bin(`/demo/files/${current}/preview-${mode}.bin`));
      previews.set(key, { width: p.width, height: p.height, channels: p.channels, data: new Uint16Array(p.body) });
    }
    if (!displays.has(key)) displays.set(key, await json<Display>(`/demo/files/${current}/display-${mode}.json`));
    return { px: previews.get(key)!, d: displays.get(key)! };
  };

  return {
    initialPath: async () => null,
    pickFile: async () => null,
    pickFolder: async () => "demo",
    listFolder: async () => (await json<{ entries: Entry[] }>("/demo/list.json")).entries,
    async thumbnail(path) {
      try {
        const p = unpack(await bin(`/demo/files/${idOf(path)}/thumb.bin`));
        return { width: p.width, height: p.height, rgba: new Uint8ClampedArray(p.body) };
      } catch {
        return null;
      }
    },
    async openFile(path) {
      current = idOf(path);
      opened = await json<Opened>(`/demo/files/${current}/open.json`);
      return opened;
    },
    display: async (mode) => (await load(mode)).d,
    preview: async (mode) => (await load(mode)).px,
    async region(mode, x, y, w, h) {
      // Crop the preview (lower resolution than the real region).
      const { px, d } = await load(mode);
      const f = d.width / px.width;
      const [x0, y0] = [Math.floor(x / f), Math.floor(y / f)];
      const [rw, rh] = [Math.max(1, Math.min(px.width - x0, Math.ceil(w / f))), Math.max(1, Math.min(px.height - y0, Math.ceil(h / f)))];
      const out = new Uint16Array(rw * rh * px.channels);
      for (let r = 0; r < rh; r++) {
        const src = ((y0 + r) * px.width + x0) * px.channels;
        out.set(px.data.subarray(src, src + rw * px.channels), r * rw * px.channels);
      }
      return { width: rw, height: rh, channels: px.channels, data: out };
    },
    async readout(x, y) {
      if (!opened?.image) return null;
      const mode = opened.image.default_mode;
      const { px, d } = await load(mode);
      const f = (d.width * d.source_step) / px.width;
      const [px_x, px_y] = [Math.min(px.width - 1, Math.floor(x / f)), Math.min(px.height - 1, Math.floor(y / f))];
      const norm = Array.from({ length: px.channels }, (_, c) => halfToFloat(px.data[(px_y * px.width + px_x) * px.channels + c]));
      const [lo, hi] = opened.image.normalized_from;
      return { x, y, norm, raw: norm.map((v) => lo + v * (hi - lo)) };
    },
    header: async (path) => json<HeaderDoc>(`/demo/files/${idOf(path)}/header.json`),
    dictionary: () => json<KeywordInfo[]>("/demo/dictionary.json"),
    // Simulated dry run so the editor can be exercised in a browser. The real
    // plan (validation, block count, consequences) comes from fittle-core.
    async planEdit(path, ops) {
      const doc = await json<HeaderDoc>(`/demo/files/${idOf(path)}/header.json`);
      const cards = doc.hdus.find((h) => h.shape.length)?.cards ?? doc.hdus[0].cards;
      const show = (v: { type: string; value?: unknown }) => (v.type === "string" ? `'${v.value}'` : String(v.value));
      const changes: Plan["changes"] = ops.map((o) => {
        if (o.op === "set") {
          const c = cards.find((x) => x.keyword === o.key && x.type !== "commentary");
          return c
            ? { kind: "modified" as const, key: o.key, before: show(c), after: show(o.value) }
            : { kind: "added" as const, key: o.key, after: show(o.value) };
        }
        if (o.op === "unset") return { kind: "removed" as const, key: o.key, before: show(cards.find((x) => x.keyword === o.key) ?? { type: "undefined" }) };
        if (o.op === "rename") return { kind: "renamed" as const, key: `${o.from} → ${o.to}` };
        return { kind: "history" as const, key: "HISTORY", after: o.text };
      });
      return {
        path,
        hdu: 0,
        changes,
        header_blocks_before: 1,
        header_blocks_after: 1,
        in_place: true,
        consequences: ["Demo mode simulates this preview; the desktop app validates and writes."],
        warnings: [],
      };
    },
    applyEdits: () => Promise.reject(new Error("Editing needs the desktop app (demo mode is read-only).")),
    scrubOps: () =>
      Promise.resolve([
        { op: "unset" as const, key: "SITELAT" },
        { op: "unset" as const, key: "SITELONG" },
      ]),
    // Export needs the file itself; the demo only approximates the plan.
    async exportPlan(spec, template) {
      const f = opened?.info.fields;
      const img = opened?.image;
      const ext = { png: "png", jpeg: "jpg", webp: "webp", avif: "avif", tiff: "tif", fits: "fits" }[spec.format.kind];
      const name = template
        .replace("{object}", f?.object?.value ?? "unknown")
        .replace("{filter}", f?.filter?.value ?? "unknown")
        .replace("{name}", idOf(current))
        .replace(/\{[a-z]+\}/g, "unknown");
      let [w, h] = [img?.width ?? 0, img?.height ?? 0];
      if (spec.long_edge) [w, h] = [Math.round((w * spec.long_edge) / Math.max(w, h)), Math.round((h * spec.long_edge) / Math.max(w, h))];
      const ch = img?.can_debayer && spec.debayer ? 3 : (img?.planes ?? 1);
      return { file_name: `${name}.${ext}`, missing: [], width: w, height: h, channels: ch, estimate_bytes: w * h * ch * 0.5 };
    },
    exportPreview: () => Promise.reject(new Error("Export preview needs the desktop app.")),
    exportImage: () => Promise.reject(new Error("Export needs the desktop app; the demo has no files.")),
    packFile: () => Promise.reject(new Error("Compression needs the desktop app; the demo has no files.")),
    // Fixture written by `fittle scan --grade --report json -o app/demo-fixtures/report.json`.
    sessionReport: () => json<Report>("/demo/report.json"),
    saveReport: () => Promise.reject(new Error("Session reports need the desktop app.")),
    moveRejects: () => Promise.reject(new Error("Moving files needs the desktop app.")),
    // Blink in the demo uses the (small) thumbnails; stretch is baked in.
    async blinkFrame(path) {
      const p = unpack(await bin(`/demo/files/${idOf(path)}/thumb.bin`));
      return { width: p.width, height: p.height, bottomUp: false, stf: [0, 0.5, 1, 0, 0.5, 1, 0, 0.5, 1], rgba: new Uint8ClampedArray(p.body) };
    },
    subStats: () => Promise.reject(new Error("Star metrics need the desktop app.")),
    // Fixture: `fittle diff <a> <b> --json > app/demo-fixtures/diff.json`.
    diffFiles: () => json<HeaderDiff>("/demo/diff.json"),
    // Fixture: `fittle keys <folder> --json > app/demo-fixtures/keys.json`.
    keywordSpread: () => json<Distribution>("/demo/keys.json"),
    batchPlan: async (paths) => ({ files: paths.length, changed: paths.length, in_place: paths.length, rewrite: 0, backup_bytes: paths.length * 4.2e6, est_seconds: 2, errors: [], notes: [], sample: [], cli: "fittle set … (demo)" }),
    batchApply: () => Promise.reject(new Error("Editing needs the desktop app.")),
    rigsList: async () => [{ name: "ZWO Seestar S50", values: { FOCALLEN: 250, APTDIA: 50, FOCRATIO: 5, XPIXSZ: 2.9, YPIXSZ: 2.9 }, builtin: true }],
    rigSave: () => Promise.reject(new Error("Saving rigs needs the desktop app.")),
    rigDelete: async () => false,
    organizePlan: () => Promise.reject(new Error("Organizing needs the desktop app.")),
    organizeApply: () => Promise.reject(new Error("Organizing needs the desktop app.")),
    // Fixture written by `fittle match-cal <lights> --library <dir> --json > app/demo-fixtures/calmatch.json`.
    matchCalibration: () => json<Matching>("/demo/calmatch.json"),
  };
}

export function halfToFloat(h: number): number {
  const s = h & 0x8000 ? -1 : 1;
  const e = (h >> 10) & 0x1f;
  const f = h & 0x3ff;
  if (e === 0) return s * 2 ** -14 * (f / 1024);
  if (e === 31) return f ? NaN : s * Infinity;
  return s * 2 ** (e - 15) * (1 + f / 1024);
}
