// Session report screen: a header-only report first (fast), then the graded
// one (a few seconds per thousand subs). Latest request wins.
import type { Report } from "../backend/types";
import { app, getBackend, notify } from "./app";
import { createStore } from "./createStore";

type ReportState = {
  open: boolean;
  folder: string | null;
  recursive: boolean;
  report: Report | null;
  grading: boolean;
  error: string | null;
  /** Night shown, or null for all nights. */
  night: string | null;
};

export const reportStore = createStore<ReportState>({
  open: false,
  folder: null,
  recursive: true,
  report: null,
  grading: false,
  error: null,
  night: null,
});

let seq = 0;

export async function openReport(folder?: string) {
  const path = folder ?? app.get().folder?.path;
  if (!path) return;
  const mine = ++seq;
  const recursive = reportStore.get().recursive;
  reportStore.set({ open: true, folder: path, report: null, grading: true, error: null, night: null });
  const b = getBackend();
  try {
    const quick = await b.sessionReport(path, recursive, false);
    if (mine !== seq) return;
    reportStore.set({ report: quick });
    const graded = await b.sessionReport(path, recursive, true);
    if (mine !== seq) return;
    reportStore.set({ report: graded });
  } catch (e) {
    if (mine === seq) reportStore.set({ error: e instanceof Error ? e.message : String(e) });
  } finally {
    if (mine === seq) reportStore.set({ grading: false });
  }
}

export function closeReport() {
  seq++;
  reportStore.set({ open: false });
}

export async function saveReport(format: "md" | "html" | "astrobin") {
  try {
    const path = await getBackend().saveReport(format);
    notify(`Saved ${path.split(/[\\/]/).pop()} in the folder`);
  } catch (e) {
    notify(e instanceof Error ? e.message : String(e), "bad");
  }
}

/** Move the suggested rejects into _rejected/ (after the user confirmed). */
export async function moveRejects() {
  const r = reportStore.get().report;
  const paths = r?.grading?.subs.filter((s) => s.reject).map((s) => s.path) ?? [];
  if (!paths.length) return;
  try {
    const moves = await getBackend().moveRejects(paths, false);
    const failed = moves.filter((m) => m.error);
    notify(
      failed.length ? `Moved ${moves.length - failed.length}; ${failed.length} not moved (${failed[0].error})` : `Moved ${moves.length} subs into _rejected/`,
      failed.length ? "bad" : "ok",
    );
    const folder = reportStore.get().folder;
    if (folder) {
      const shown = app.get().folder;
      if (shown?.path === folder) {
        const entries = await getBackend().listFolder(folder);
        app.set({ folder: { ...shown, entries } });
      }
      void openReport(folder);
    }
  } catch (e) {
    notify(e instanceof Error ? e.message : String(e), "bad");
  }
}
