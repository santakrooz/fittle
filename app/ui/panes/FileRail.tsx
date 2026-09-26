import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useEffect, useRef } from "react";
import type { Entry } from "../backend/types";
import { Button, Chip } from "../ds";
import { group, num, shortName } from "../format";
import { app, getBackend, openFolder, visibleEntries, type Filter } from "../state/app";
import { openReport } from "../state/report";
import { clearSelection, openBatch, railClick, selectAll } from "../state/batch";
import { openDiff } from "./HeaderDiff";

/** Row pitch in the rail, px. */
const ROW = 52;

const FILTERS: { value: Filter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "lights", label: "Lights" },
  { value: "stacks", label: "Stacks" },
  { value: "calib", label: "Calib" },
];

function detail(e: Entry): string {
  if (e.error && !e.label) return "Unreadable";
  const kind = e.integrated
    ? e.frame === "light" || !e.frame
      ? `Stack${e.stack_count ? ` · ${group(e.stack_count)}` : ""}`
      : (e.label ?? "Master")
    : (e.label?.replace(/ (sub|frame)$/, "") ?? "FITS");
  const exp = e.exposure_s != null && !e.integrated ? ` · ${num(e.exposure_s, 2)} s` : "";
  const filter = e.filter ? ` · ${e.filter}` : "";
  return `${kind}${exp}${filter}`;
}

/** At most three thumbnails in flight, so they never compete with opening a file. */
const queue: (() => Promise<void>)[] = [];
let running = 0;
function enqueue(job: () => Promise<void>) {
  queue.push(job);
  pump();
}
function pump() {
  while (running < 3 && queue.length) {
    const job = queue.shift()!;
    running++;
    job().finally(() => {
      running--;
      pump();
    });
  }
}

const Thumb = memo(function Thumb({ path }: { path: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let live = true;
    enqueue(async () => {
      if (!live) return; // scrolled away before its turn
      const t = await getBackend().thumbnail(path);
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
  return <canvas ref={ref} className="ft-thumb" aria-hidden="true" />;
});

export function FileRail() {
  const folder = app.use((s) => s.folder);
  const filter = app.use((s) => s.filter);
  const current = app.use((s) => s.current);
  const selection = app.use((s) => s.selection);
  const list = visibleEntries({ folder, filter });
  const scroller = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: list.length, getScrollElement: () => scroller.current, estimateSize: () => ROW, overscan: 8 });

  // Keep the open file in view when stepping with the arrow keys; only
  // scroll when it is actually out of view.
  useEffect(() => {
    const el = scroller.current;
    const i = list.findIndex((e) => e.path === current);
    if (!el || i < 0) return;
    const top = i * ROW;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW > el.scrollTop + el.clientHeight) el.scrollTop = top + ROW - el.clientHeight;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current, filter, list.length]);

  const choose = async () => {
    const p = await getBackend().pickFolder();
    if (p) openFolder(p);
  };

  return (
    <aside className="rail" aria-label="Files">
      <div className="rail-head">
        <span className="ft-label">{folder ? `Folder · ${group(folder.entries.length)}` : "No folder"}</span>
        {folder && (
          <button type="button" className="rail-scan" onClick={() => void openReport(folder.path)} title="Analyze this folder: nights, integration, consistency checks and sub grades (includes subfolders)">
            Analyze
          </button>
        )}
        <Button variant="ghost" icon="folder" onClick={choose}>
          Open
        </Button>
      </div>
      {folder && selection.length > 1 && (
        <div className="rail-selection">
          <span>{group(selection.length)} selected</span>
          {selection.length === 2 && (
            <button type="button" className="rail-scan" onClick={() => void openDiff()} title="Compare headers (D)">Diff</button>
          )}
          <button type="button" className="rail-scan" onClick={() => void openBatch()}>Edit…</button>
          <button type="button" className="rail-link" onClick={clearSelection}>Clear</button>
        </div>
      )}
      {folder && (
        <div className="rail-filters" role="group" aria-label="Filter files">
          {FILTERS.map((f) => (
            <Chip key={f.value} on={filter === f.value} onClick={() => app.set({ filter: f.value })}>
              {f.label}
            </Chip>
          ))}
          <button type="button" className="rail-link" onClick={selectAll} title="Select every file shown (⌘/Ctrl-click and Shift-click also select)">
            Select all
          </button>
        </div>
      )}
      <div ref={scroller} className="rail-list">
        {!folder && <p className="rail-empty">Open a folder to browse its FITS files, or drop one on the window.</p>}
        {folder && list.length === 0 && <p className="rail-empty">No files match this filter.</p>}
        <div style={{ height: v.getTotalSize(), position: "relative" }}>
          {v.getVirtualItems().map((row) => {
            const e = list[row.index];
            // Quality grading (green/amber) arrives with the sub grader in M6.
            const status = e.error ? "bad" : "none";
            return (
              <button
                key={e.path}
                type="button"
                className="ft-file"
                aria-selected={selection.length ? selection.includes(e.path) : e.path === current}
                title={e.name}
                style={{ position: "absolute", top: 0, left: 0, transform: `translateY(${row.start}px)` }}
                onClick={(ev) => railClick(e.path, ev)}
              >
                <Thumb path={e.path} />
                <span style={{ minWidth: 0 }}>
                  <span className="ft-fn" style={{ display: "block" }}>
                    {shortName(e.name)}
                  </span>
                  <span className="ft-fs">
                    <i className={`ft-dot ${status}`} />
                    {detail(e)}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </aside>
  );
}
