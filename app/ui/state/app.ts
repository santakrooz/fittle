// App state and actions. Components read slices with `app.use(s => …)`;
// actions talk to the Backend and are the only writers.
import type {
  Backend,
  Display,
  Entry,
  HeaderDoc,
  KeywordInfo,
  Mode,
  Opened,
  Pixels,
  Readout,
} from "../backend/types";
import { createStore } from "./createStore";
import { DEFAULT_STRETCH, autoStf, type Stretch } from "./stretch";

export type Filter = "all" | "lights" | "stacks" | "calib";
export type Tab = "overview" | "header" | "histogram";

export const PREVIEW_EDGE = 2560;

type AppState = {
  folder: { path: string; name: string; entries: Entry[] } | null;
  filter: Filter;
  current: string | null;
  opened: Opened | null;
  loading: boolean;
  error: string | null;
  mode: Mode;
  display: Display | null;
  preview: Pixels | null;
  stretch: Stretch;
  tab: Tab;
  palette: boolean;
  header: HeaderDoc | null;
  headerQuery: string;
  dictionary: Map<string, KeywordInfo>;
  /** Short result message shown as a toast. */
  notice: { text: string; tone: "ok" | "bad" } | null;
};

export const app = createStore<AppState>({
  folder: null,
  filter: "all",
  current: null,
  opened: null,
  loading: false,
  error: null,
  mode: "raw",
  display: null,
  preview: null,
  stretch: DEFAULT_STRETCH,
  tab: "overview",
  palette: false,
  header: null,
  headerQuery: "",
  dictionary: new Map(),
  notice: null,
});

/** Pixel under the cursor; updated at pointer rate, read only by the HUD. */
export const hover = createStore<{ readout: Readout | null; zoom: number }>({ readout: null, zoom: 1 });

let backend: Backend;
let openSeq = 0;

export function init(b: Backend) {
  backend = b;
  backend.dictionary().then((list) => app.set({ dictionary: new Map(list.map((k) => [k.keyword, k])) }));
}

export const getBackend = () => backend;

const baseName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
const dirName = (p: string) => p.replace(/[\\/][^\\/]*$/, "");

export function visibleEntries(s: Pick<AppState, "folder" | "filter">): Entry[] {
  const all = s.folder?.entries ?? [];
  switch (s.filter) {
    case "lights":
      return all.filter((e) => e.frame === "light" && !e.integrated);
    case "stacks":
      return all.filter((e) => e.integrated);
    case "calib":
      return all.filter((e) => e.frame && !["light", "unknown"].includes(e.frame));
    default:
      return all;
  }
}

export async function openFolder(path: string, select?: string) {
  const entries = await backend.listFolder(path);
  app.set({ folder: { path, name: baseName(path), entries }, filter: "all" });
  const first = select ?? entries.find((e) => !e.error)?.path;
  if (first) await openFile(first, false);
}

/** Open a file; also lists its folder unless it is already shown. */
export async function openFile(path: string, listFolder = true) {
  const seq = ++openSeq;
  const t0 = performance.now();
  app.set({ current: path, loading: true, error: null });
  try {
    if (listFolder && !path.startsWith("demo:") && app.get().folder?.path !== dirName(path)) {
      backend.listFolder(dirName(path)).then((entries) =>
        app.set({ folder: { path: dirName(path), name: baseName(dirName(path)), entries } }),
      );
    }
    const opened = await backend.openFile(path);
    if (seq !== openSeq) return;
    const mode = opened.image?.default_mode ?? "raw";
    app.set({ opened, mode, header: null, error: opened.image_error ?? null });
    backend.header(path).then((header) => seq === openSeq && app.set({ header }));
    if (opened.image) await loadMode(mode, seq);
    // Pixels are on the GPU after the next frame.
    requestAnimationFrame(() =>
      requestAnimationFrame(() => backend.log?.(`open → first frame: ${(performance.now() - t0).toFixed(0)} ms  ${path}`)),
    );
  } catch (e) {
    if (seq === openSeq) app.set({ error: String(e), opened: null, display: null, preview: null });
  } finally {
    if (seq === openSeq) app.set({ loading: false });
  }
}

async function loadMode(mode: Mode, seq = openSeq) {
  const [display, preview] = await Promise.all([backend.display(mode), backend.preview(mode, PREVIEW_EDGE)]);
  if (seq !== openSeq) return;
  const s = app.get().stretch;
  // A manual stretch starts from the new file's auto values.
  const stretch = s.kind === "manual" ? { ...s, manual: autoStf(display, s.linked) } : s;
  app.set({ mode, display, preview, stretch });
}

export async function setMode(mode: Mode) {
  if (!app.get().opened?.image || app.get().mode === mode) return;
  app.set({ loading: true });
  try {
    await loadMode(mode);
  } finally {
    app.set({ loading: false });
  }
}

export function setStretch(patch: Partial<Stretch>) {
  app.set((s) => {
    const next = { ...s.stretch, ...patch };
    // Entering manual copies the current auto curve so nothing jumps.
    if (patch.kind === "manual" && s.stretch.kind !== "manual" && !patch.manual && s.display) {
      next.manual = autoStf(s.display, s.stretch.linked);
    }
    return { stretch: next };
  });
}

/** Step through the visible files (arrow keys). */
export function step(delta: number) {
  const s = app.get();
  const list = visibleEntries(s);
  if (!list.length) return;
  const i = list.findIndex((e) => e.path === s.current);
  const next = list[Math.max(0, Math.min(list.length - 1, (i < 0 ? 0 : i) + delta))];
  if (next && next.path !== s.current) openFile(next.path, false);
}

// ---- view commands (handled by the Stage) ---------------------------------

export type ViewCommand = "fit" | "one" | "in" | "out";
const viewListeners = new Set<(c: ViewCommand) => void>();
export const onViewCommand = (l: (c: ViewCommand) => void) => {
  viewListeners.add(l);
  return () => viewListeners.delete(l);
};
export const view = (c: ViewCommand) => viewListeners.forEach((l) => l(c));

let noticeTimer: ReturnType<typeof setTimeout> | undefined;
export function notify(text: string, tone: "ok" | "bad" = "ok") {
  clearTimeout(noticeTimer);
  app.set({ notice: { text, tone } });
  noticeTimer = setTimeout(() => app.set({ notice: null }), 6000);
}

const mb = (b: number) => (b >= 1e6 ? `${(b / 1e6).toFixed(1)} MB` : `${Math.round(b / 1e3)} kB`);

/** Compress (fpack) or expand (funpack) the current file into a new file beside it. */
export async function packCurrent(unpack: boolean) {
  const path = app.get().current;
  if (!path) return;
  notify(unpack ? "Expanding…" : "Compressing…");
  try {
    const r = await backend.packFile(path, unpack);
    notify(`Saved ${baseName(r.path)} · ${mb(r.bytes_in)} → ${mb(r.bytes_out)}, verified`);
    const folder = app.get().folder;
    if (folder && folder.path === dirName(r.path)) {
      const entries = await backend.listFolder(folder.path);
      app.set({ folder: { ...folder, entries } });
    }
  } catch (e) {
    notify(e instanceof Error ? e.message : String(e), "bad");
  }
}
