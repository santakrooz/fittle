import { useEffect, useRef, useState } from "react";
import { baseName, group } from "../format";
import { app, getBackend, openFile, visibleEntries } from "../state/app";
import { createStore } from "../state/createStore";
import { reportStore } from "../state/report";

const KEY = "fittle.filmstrip";
function saved() {
  try {
    return localStorage.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}
export const filmstrip = createStore<{ on: boolean }>({ on: saved() });
export function toggleFilmstrip() {
  const on = !filmstrip.get().on;
  filmstrip.set({ on });
  try {
    localStorage.setItem(KEY, on ? "on" : "off");
  } catch {
    /* this session only */
  }
}

const CELL = 102; // thumbnail + gap, px

function Frame({ path, on, rejected }: { path: string; on: boolean; rejected: boolean }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let live = true;
    getBackend()
      .thumbnail(path)
      .then((t) => {
        const c = ref.current;
        if (!t || !c || !live) return;
        c.width = t.width;
        c.height = t.height;
        c.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(t.rgba), t.width, t.height), 0, 0);
      });
    return () => {
      live = false;
    };
  }, [path]);
  return (
    <button
      type="button"
      className={`strip-thumb ${on ? "on" : ""} ${rejected ? "rej" : ""}`}
      onClick={() => void openFile(path, false)}
      title={baseName(path)}
      aria-current={on}
    >
      <canvas ref={ref} aria-hidden="true" />
    </button>
  );
}

/** Thumbnails around the open file (screen 1). */
export function Filmstrip() {
  const on = filmstrip.use((s) => s.on);
  const folder = app.use((s) => s.folder);
  const filter = app.use((s) => s.filter);
  const current = app.use((s) => s.current);
  const grading = reportStore.use((s) => s.report?.grading);
  const box = useRef<HTMLDivElement>(null);
  const [fit, setFit] = useState(10);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setFit(Math.max(3, Math.floor((el.clientWidth - 80) / CELL))));
    ro.observe(el);
    return () => ro.disconnect();
  }, [on, folder]);

  const list = visibleEntries({ folder, filter }).filter((e) => !e.error);
  if (!on || !folder || list.length < 2) return null;
  const paths = list.map((e) => e.path);
  const i = Math.max(0, paths.indexOf(current ?? ""));
  const half = Math.floor(fit / 2);
  const lo = Math.max(0, Math.min(i - half, paths.length - fit));
  const shown = paths.slice(lo, lo + fit);
  const rejected = new Set(grading?.subs.filter((s) => s.reject).map((s) => s.path) ?? []);
  const rest = paths.length - shown.length;

  return (
    <div className="filmstrip" ref={box} aria-label="Filmstrip">
      {shown.map((p) => (
        <Frame key={p} path={p} on={p === current} rejected={rejected.has(p)} />
      ))}
      {rest > 0 && <span className="strip-more">+{group(rest)}</span>}
    </div>
  );
}
