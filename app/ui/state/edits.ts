// Staged header edits for the open file. Ops are kept in order; the plan (the
// diff, block count and consequences) always comes from the Rust core.
import type { EditOptions, FileResult, Op, Plan } from "../backend/types";
import { app, getBackend, openFile, visibleEntries } from "./app";
import { createStore } from "./createStore";

type EditState = {
  editing: boolean;
  ops: Op[];
  plan: Plan | null;
  error: string | null;
  options: EditOptions;
  /** Apply to every visible file in the folder rail. */
  batch: boolean;
  busy: boolean;
  /** Result of the last write, shown until the next change. */
  result: { ok: number; failed: FileResult[]; changes: number } | null;
};

export const edits = createStore<EditState>({
  editing: false,
  ops: [],
  plan: null,
  error: null,
  options: { backup: true, history: true, checksum: true, hdu: null },
  batch: false,
  busy: false,
  result: null,
});

let seq = 0;

async function replan() {
  const s = edits.get();
  const path = app.get().current;
  const n = ++seq;
  if (!path || s.ops.length === 0) {
    edits.set({ plan: null, error: null });
    return;
  }
  try {
    const plan = await getBackend().planEdit(path, s.ops, s.options);
    if (n === seq) edits.set({ plan, error: null });
  } catch (e) {
    if (n === seq) edits.set({ error: String(e).replace(/^Error: /, "") });
  }
}

/** Replace any staged op for the same keyword, then add this one. */
export function stage(op: Op) {
  const key = op.op === "rename" ? op.from : op.op === "history" ? null : op.key;
  edits.set((s) => ({
    ops: [...s.ops.filter((o) => key === null || (o.op === "rename" ? o.from : o.op === "history" ? null : o.key) !== key), op],
    result: null,
  }));
  replan();
}

export function unstage(key: string) {
  edits.set((s) => ({ ops: s.ops.filter((o) => !("key" in o && o.key === key) && !(o.op === "rename" && o.from === key)) }));
  replan();
}

export function discard() {
  edits.set({ ops: [], plan: null, error: null, result: null });
}

export function setEditing(on: boolean) {
  if (!on) discard();
  edits.set({ editing: on });
}

export function setOption(patch: Partial<EditOptions>) {
  edits.set((s) => ({ options: { ...s.options, ...patch } }));
  replan();
}

export async function stageScrub() {
  const path = app.get().current;
  if (!path) return;
  for (const op of await getBackend().scrubOps(path)) stage(op);
}

/** Paths a write applies to: the open file, or every visible file. */
export function targets(): string[] {
  const s = app.get();
  if (!s.current) return [];
  return edits.get().batch ? visibleEntries(s).map((e) => e.path) : [s.current];
}

export async function write() {
  const s = edits.get();
  const paths = targets();
  if (!s.ops.length || !paths.length) return;
  edits.set({ busy: true, error: null });
  try {
    const results = await getBackend().applyEdits(paths, s.ops, s.options);
    const failed = results.filter((r) => r.error);
    edits.set({
      ops: [],
      plan: null,
      result: { ok: results.length - failed.length, failed, changes: s.plan?.changes.length ?? 0 },
    });
    // Re-read so every panel (and the rail, after a batch) shows what is on disk.
    const current = app.get().current;
    if (current) await openFile(current, false);
    const folder = app.get().folder;
    if (paths.length > 1 && folder) {
      const entries = await getBackend().listFolder(folder.path);
      app.set({ folder: { ...folder, entries } });
    }
  } catch (e) {
    edits.set({ error: String(e).replace(/^Error: /, "") });
  } finally {
    edits.set({ busy: false });
  }
}

// A different file invalidates staged edits.
let lastPath = app.get().current;
app.subscribe(() => {
  const p = app.get().current;
  if (p !== lastPath) {
    lastPath = p;
    if (!edits.get().busy) discard();
  }
});
