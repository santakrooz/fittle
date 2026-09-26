// Batch editor (screen 6): select files in the rail, see how each keyword
// varies, queue edits (or a rig profile), dry-run, then apply.
import type { BatchPlan, Distribution, EditOptions, Op, Rig } from "../backend/types";
import { app, getBackend, notify, openFile, visibleEntries } from "./app";
import { createStore } from "./createStore";

export const batch = createStore<{
  open: boolean;
  dist: Distribution | null;
  ops: Op[];
  plan: BatchPlan | null;
  rigs: Rig[];
  busy: boolean;
  error: string | null;
  options: EditOptions;
}>({
  open: false,
  dist: null,
  ops: [],
  plan: null,
  rigs: [],
  busy: false,
  error: null,
  options: { backup: true, history: true, checksum: true, hdu: null },
});

let anchor: string | null = null;

/** Rail click: plain opens; ⌘/Ctrl toggles; Shift selects a range. */
export function railClick(path: string, e: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) {
  const s = app.get();
  const list = visibleEntries(s).map((x) => x.path);
  if (e.shiftKey) {
    const from = anchor ?? s.current ?? path;
    const [a, b] = [list.indexOf(from), list.indexOf(path)].sort((x, y) => x - y);
    if (a >= 0 && b >= 0) app.set({ selection: list.slice(a, b + 1) });
    return;
  }
  if (e.metaKey || e.ctrlKey) {
    const sel = s.selection.length ? s.selection : s.current ? [s.current] : [];
    app.set({ selection: sel.includes(path) ? sel.filter((p) => p !== path) : [...sel, path] });
    anchor = path;
    return;
  }
  anchor = path;
  app.set({ selection: [] });
  void openFile(path, false);
}

export const selectAll = () => app.set({ selection: visibleEntries(app.get()).filter((e) => !e.error).map((e) => e.path) });
export const clearSelection = () => app.set({ selection: [] });

export async function openBatch() {
  const paths = app.get().selection;
  if (paths.length < 1) return;
  batch.set({ open: true, dist: null, plan: null, error: null, ops: [] });
  const b = getBackend();
  try {
    const [dist, rigs] = await Promise.all([b.keywordSpread(paths), b.rigsList()]);
    batch.set({ dist, rigs });
  } catch (e) {
    batch.set({ error: e instanceof Error ? e.message : String(e) });
  }
}

export const closeBatch = () => batch.set({ open: false });

let seq = 0;
async function replan() {
  const { ops, options } = batch.get();
  const paths = app.get().selection;
  const mine = ++seq;
  if (!ops.length) return batch.set({ plan: null });
  try {
    const plan = await getBackend().batchPlan(paths, ops, options);
    if (mine === seq) batch.set({ plan, error: null });
  } catch (e) {
    if (mine === seq) batch.set({ error: e instanceof Error ? e.message : String(e) });
  }
}

export function queue(op: Op) {
  const key = (o: Op) => (o.op === "rename" ? o.from : o.op === "history" ? "" : o.key);
  // One pending op per keyword: the newest wins.
  batch.set((s) => ({ ops: [...s.ops.filter((o) => key(o) !== key(op)), op] }));
  void replan();
}
export function unqueue(i: number) {
  batch.set((s) => ({ ops: s.ops.filter((_, k) => k !== i) }));
  void replan();
}
export function setOptions(patch: Partial<EditOptions>) {
  batch.set((s) => ({ options: { ...s.options, ...patch } }));
  void replan();
}

export function queueRig(rig: Rig) {
  for (const [key, v] of Object.entries(rig.values)) {
    const value =
      typeof v === "boolean"
        ? ({ type: "logical", value: v } as const)
        : typeof v === "number"
          ? Number.isInteger(v) && !["FOCALLEN", "APTDIA", "FOCRATIO", "XPIXSZ", "YPIXSZ"].includes(key)
            ? ({ type: "integer", value: v } as const)
            : ({ type: "float", value: v } as const)
          : ({ type: "string", value: v } as const);
    queue({ op: "set", key, value, comment: `rig: ${rig.name}` });
  }
}

export async function saveRig(name: string) {
  const from = app.get().selection[0];
  if (!from || !name.trim()) return;
  try {
    await getBackend().rigSave(name.trim(), from, []);
    batch.set({ rigs: await getBackend().rigsList() });
    notify(`Saved rig “${name.trim()}”`);
  } catch (e) {
    notify(e instanceof Error ? e.message : String(e), "bad");
  }
}

export async function applyBatch() {
  const { ops, options } = batch.get();
  const paths = app.get().selection;
  batch.set({ busy: true });
  try {
    const failed = await getBackend().batchApply(paths, ops, options);
    notify(failed.length ? `${failed.length} files not written: ${failed[0][1]}` : `Updated ${paths.length} files`, failed.length ? "bad" : "ok");
    batch.set({ ops: [], plan: null, dist: await getBackend().keywordSpread(paths) });
    const cur = app.get().current;
    if (cur && paths.includes(cur)) void openFile(cur, false);
  } catch (e) {
    batch.set({ error: e instanceof Error ? e.message : String(e) });
  } finally {
    batch.set({ busy: false });
  }
}
