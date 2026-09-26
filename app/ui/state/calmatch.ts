// Match calibration screen (screen 9). The library folder is remembered per
// viewer; the lights are the open folder.
import type { Matching } from "../backend/types";
import { app, getBackend } from "./app";
import { createStore } from "./createStore";

const KEY = "fittle.calibrationLibrary";

function saved(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export const calStore = createStore<{
  open: boolean;
  lights: string | null;
  library: string | null;
  result: Matching | null;
  busy: boolean;
  error: string | null;
}>({ open: false, lights: null, library: saved(), result: null, busy: false, error: null });

let seq = 0;

export async function openCalMatch(lights?: string) {
  const l = lights ?? app.get().folder?.path ?? null;
  calStore.set({ open: true, lights: l, result: null, error: null });
  if (!calStore.get().library) await chooseLibrary();
  else void run();
}

export async function chooseLibrary() {
  const d = await getBackend().pickFolder();
  if (!d) return;
  try {
    localStorage.setItem(KEY, d);
  } catch {
    /* remembered for this session only */
  }
  calStore.set({ library: d });
  void run();
}

async function run() {
  const { lights, library } = calStore.get();
  if (!lights || !library) return;
  const mine = ++seq;
  calStore.set({ busy: true, error: null });
  try {
    const result = await getBackend().matchCalibration(lights, library);
    if (mine === seq) calStore.set({ result });
  } catch (e) {
    if (mine === seq) calStore.set({ error: e instanceof Error ? e.message : String(e) });
  } finally {
    if (mine === seq) calStore.set({ busy: false });
  }
}

export function closeCalMatch() {
  seq++;
  calStore.set({ open: false });
}
