import { useEffect } from "react";
import type { Backend } from "./backend/types";
import { ExportModal } from "./panes/ExportModal";
import { FileRail } from "./panes/FileRail";
import { Inspector } from "./panes/Inspector";
import { Palette } from "./panes/Palette";
import { SessionReport } from "./panes/SessionReport";
import { CalMatch } from "./panes/CalMatch";
import { Blink } from "./panes/Blink";
import { blink, openBlink } from "./state/blink";
import { calStore } from "./state/calmatch";
import { Hud, StageToolbar } from "./panes/StageChrome";
import { TitleBar } from "./panes/TitleBar";
import { app, init, openFile, openFolder, setMode, setStretch, step, view } from "./state/app";
import { edits } from "./state/edits";
import { exporter, openExport } from "./state/exporter";
import { reportStore } from "./state/report";
import { Stage } from "./viewer/Stage";

/** React's development mode runs effects twice; open the startup file once. */
let started = false;

function typing(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
}

export function App({ backend, demo }: { backend: Backend; demo?: boolean }) {
  useEffect(() => {
    init(backend);
    if (!started) {
      started = true;
      if (demo) openFolder("demo");
      else
        backend.initialPath().then((p) => {
          if (p?.dir) void openFolder(p.path);
          else if (p) void openFile(p.path);
        });
    }

    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        app.set((s) => ({ palette: !s.palette }));
        return;
      }
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "e" && !app.get().palette) {
        e.preventDefault();
        openExport();
        return;
      }
      if (typing(e) || app.get().palette || exporter.get().open || reportStore.get().open || calStore.get().open || blink.get().open || e.metaKey || e.ctrlKey || e.altKey) return;
      // In the editor, arrows must not switch files under staged edits.
      if (edits.get().editing && app.get().tab === "header") return;
      const s = app.get();
      const k = e.key;
      if (k === "ArrowDown" || k === "ArrowRight") (e.preventDefault(), step(1));
      else if (k === "ArrowUp" || k === "ArrowLeft") (e.preventDefault(), step(-1));
      else if (k === "f") view("fit");
      else if (k === "1") view("one");
      else if (k === "+" || k === "=") view("in");
      else if (k === "-") view("out");
      else if (k === "a") setStretch({ kind: "auto" });
      else if (k === "l") setStretch({ kind: "linear" });
      else if (k === "h") setStretch({ kind: "asinh" });
      else if (k === "c") setStretch({ clipping: !s.stretch.clipping });
      else if (k === "b" && s.folder) openBlink();
      else if (k === "d" && s.opened?.image?.can_debayer) setMode(s.mode === "debayer" ? "raw" : "debayer");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [backend, demo]);

  const loading = app.use((s) => s.loading);
  const error = app.use((s) => s.error);
  const hasImage = app.use((s) => !!s.preview && !!s.opened?.image);
  const current = app.use((s) => s.current);
  const tab = app.use((s) => s.tab);
  const editing = edits.use((s) => s.editing) && tab === "header";
  const notice = app.use((s) => s.notice);

  return (
    <div className={`app ${editing ? "editing" : ""}`}>
      <TitleBar />
      <FileRail />
      <main className="stage" aria-busy={loading}>
        <Stage />
        <StageToolbar />
        <Hud />
        {!hasImage && (
          <div className="stage-empty">
            {error ? <p className="stage-error">{error}</p> : <p>{loading || current ? "Reading…" : "Open a FITS file or a folder to begin."}</p>}
          </div>
        )}
        {loading && hasImage && <div className="stage-busy" aria-hidden="true" />}
      </main>
      <Inspector />
      <SessionReport />
      <CalMatch />
      <Blink />
      <Palette />
      <ExportModal />
      {notice && (
        <div className={`toast ${notice.tone}`} role="status" aria-live="polite">
          {notice.text}
        </div>
      )}
    </div>
  );
}
