// Blink (screen 8): step or play through the folder's files with a locked
// stretch, and mark keeps/rejects. Marks live here until the user moves the
// rejects (an explicit, confirmed action); nothing is written before that.
import type { BlinkFrame, FrameStats, SubGrade } from "../backend/types";
import { app, getBackend, notify, visibleEntries } from "./app";
import { createStore } from "./createStore";
import { reportStore } from "./report";

export const FPS = [2, 4, 6, 8, 10];
const EDGE = 1400;

type BlinkState = {
  open: boolean;
  paths: string[];
  index: number;
  playing: boolean;
  fps: number;
  lock: boolean;
  /** Locked stretch (from the first frame shown). */
  stf: number[] | null;
  frame: BlinkFrame | null;
  error: string | null;
  marks: Record<string, "keep" | "reject">;
  stats: Record<string, FrameStats | "loading" | "error">;
};

export const blink = createStore<BlinkState>({
  open: false,
  paths: [],
  index: 0,
  playing: false,
  fps: 6,
  lock: true,
  stf: null,
  frame: null,
  error: null,
  marks: {},
  stats: {},
});

/** Frames kept in memory (recent and prefetched). */
const cache = new Map<string, Promise<BlinkFrame>>();
const CACHE = 24;

function load(path: string): Promise<BlinkFrame> {
  const s = blink.get();
  const key = `${path}|${s.lock ? (s.stf ?? []).join(",") : "auto"}`;
  let p = cache.get(key);
  if (!p) {
    p = getBackend().blinkFrame(path, EDGE, s.lock && s.stf ? s.stf : undefined);
    cache.set(key, p);
    p.catch(() => cache.delete(key));
    while (cache.size > CACHE) cache.delete(cache.keys().next().value!);
  }
  return p;
}

/** The report's grade for a sub, if a graded report is loaded. */
export function gradeOf(path: string): SubGrade | undefined {
  return reportStore.get().report?.grading?.subs.find((s) => s.path === path);
}

let seq = 0;
async function show(index: number) {
  const s = blink.get();
  if (!s.paths.length) return;
  const i = (index + s.paths.length) % s.paths.length;
  const path = s.paths[i];
  const mine = ++seq;
  blink.set({ index: i });
  try {
    const f = await load(path);
    if (mine !== seq) return;
    blink.set({ frame: f, error: null, ...(s.lock && !s.stf ? { stf: f.stf } : {}) });
    // Prefetch the next few in the playing direction.
    for (let k = 1; k <= 3; k++) void load(s.paths[(i + k) % s.paths.length]).catch(() => {});
  } catch (e) {
    if (mine === seq) blink.set({ error: e instanceof Error ? e.message : String(e) });
  }
  if (!gradeOf(path) && !blink.get().stats[path]) {
    blink.set((b) => ({ stats: { ...b.stats, [path]: "loading" } }));
    getBackend()
      .subStats(path)
      .then((st) => blink.set((b) => ({ stats: { ...b.stats, [path]: st } })))
      .catch(() => blink.set((b) => ({ stats: { ...b.stats, [path]: "error" } })));
  }
}

let timer: ReturnType<typeof setInterval> | undefined;
function schedule() {
  clearInterval(timer);
  const s = blink.get();
  if (s.open && s.playing) timer = setInterval(() => void show(blink.get().index + 1), 1000 / s.fps);
}

export function openBlink() {
  const a = app.get();
  const list = visibleEntries(a).filter((e) => !e.error);
  if (!list.length) return;
  const paths = list.map((e) => e.path);
  const start = Math.max(0, paths.indexOf(a.current ?? ""));
  // Seed marks from a graded report's suggestions (still only marks).
  const marks: BlinkState["marks"] = {};
  for (const p of paths) if (gradeOf(p)?.reject) marks[p] = "reject";
  cache.clear();
  blink.set({ open: true, paths, index: start, playing: false, stf: null, frame: null, error: null, marks });
  void show(start);
}

export function closeBlink() {
  blink.set({ open: false, playing: false });
  schedule();
  cache.clear();
}

export const step = (d: number) => {
  blink.set({ playing: false });
  schedule();
  void show(blink.get().index + d);
};
export function togglePlay() {
  blink.set((s) => ({ playing: !s.playing }));
  schedule();
}
export function cycleFps() {
  blink.set((s) => ({ fps: FPS[(FPS.indexOf(s.fps) + 1) % FPS.length] }));
  schedule();
}
export function toggleLock() {
  cache.clear();
  blink.set((s) => ({ lock: !s.lock, stf: null }));
  void show(blink.get().index);
}
export function jump(i: number) {
  blink.set({ playing: false });
  schedule();
  void show(i);
}

export function mark(kind: "keep" | "reject") {
  const s = blink.get();
  const path = s.paths[s.index];
  if (!path) return;
  const marks = { ...s.marks };
  if (marks[path] === kind) delete marks[path];
  else marks[path] = kind;
  blink.set({ marks });
}

/** Move every sub marked reject into _rejected/ (after the user confirmed). */
export async function moveMarked() {
  const s = blink.get();
  const paths = Object.entries(s.marks)
    .filter(([, m]) => m === "reject")
    .map(([p]) => p);
  if (!paths.length) return;
  const moves = await getBackend().moveRejects(paths, false);
  const moved = new Set(moves.filter((m) => !m.error).map((m) => m.from));
  const failed = moves.length - moved.size;
  notify(failed ? `Moved ${moved.size}; ${failed} not moved` : `Moved ${moved.size} subs into _rejected/`, failed ? "bad" : "ok");
  const rest = s.paths.filter((p) => !moved.has(p));
  const marks = Object.fromEntries(Object.entries(s.marks).filter(([p]) => !moved.has(p)));
  blink.set({ paths: rest, marks, index: Math.min(s.index, Math.max(rest.length - 1, 0)) });
  const folder = app.get().folder;
  if (folder) {
    const entries = await getBackend().listFolder(folder.path);
    app.set({ folder: { ...folder, entries } });
  }
  if (rest.length) void show(blink.get().index);
  else closeBlink();
}
