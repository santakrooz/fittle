import { useEffect, useRef, useState } from "react";
import type { FrameStats } from "../backend/types";
import { Button, Kbd } from "../ds";
import { baseName, group } from "../format";
import { getBackend, openFile } from "../state/app";
import { reportStore } from "../state/report";
import { blink, closeBlink, cycleFps, gradeOf, jump, mark, moveMarked, step, toggleLock, togglePlay } from "../state/blink";

const STRIP = 7;

function Thumb({ path, on, rejected, onClick }: { path: string; on: boolean; rejected: boolean; onClick: () => void }) {
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
    <button type="button" className={`strip-thumb ${on ? "on" : ""} ${rejected ? "rej" : ""}`} onClick={onClick} title={baseName(path)} aria-current={on}>
      <canvas ref={ref} aria-hidden="true" />
    </button>
  );
}

function time(date?: string) {
  return date?.slice(11, 19) ?? "";
}

export function Blink() {
  const open = blink.use((s) => s.open);
  const paths = blink.use((s) => s.paths);
  const index = blink.use((s) => s.index);
  const playing = blink.use((s) => s.playing);
  const fps = blink.use((s) => s.fps);
  const lock = blink.use((s) => s.lock);
  const frame = blink.use((s) => s.frame);
  const error = blink.use((s) => s.error);
  const marks = blink.use((s) => s.marks);
  const statsMap = blink.use((s) => s.stats);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [confirm, setConfirm] = useState(false);
  const groups = reportStore.use((s) => s.report?.grading?.groups);

  useEffect(() => {
    const c = canvas.current;
    if (!c || !frame) return;
    c.width = frame.width;
    c.height = frame.height;
    c.getContext("2d")!.putImageData(new ImageData(frame.rgba, frame.width, frame.height), 0, 0);
  }, [frame]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const k = e.key;
      if (k === "ArrowRight" || k === "ArrowDown") (e.preventDefault(), step(1));
      else if (k === "ArrowLeft" || k === "ArrowUp") (e.preventDefault(), step(-1));
      else if (k === " ") (e.preventDefault(), togglePlay());
      else if (k === "r" || k === "R") mark("reject");
      else if (k === "k" || k === "K") mark("keep");
      else if (k === "Escape" || k === "b" || k === "B") closeBlink();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  if (!open) return null;
  const path = paths[index];
  const grade = path ? gradeOf(path) : undefined;
  const live = path ? statsMap[path] : undefined;
  const st: FrameStats | undefined = grade?.stats ?? (typeof live === "object" ? live : undefined);
  const m = path ? marks[path] : undefined;
  const rejects = Object.values(marks).filter((v) => v === "reject").length;
  const reasons = grade?.reasons ?? [];
  const median = grade ? groups?.[grade.group]?.median_stars : undefined;
  const trail = !!st?.trail;
  const lo = Math.max(0, Math.min(index - STRIP, paths.length - 2 * STRIP - 1));
  const shown = paths.slice(lo, lo + 2 * STRIP + 1);
  const status = m === "reject" ? ["Rejected", "bad"] : m === "keep" ? ["Kept", "ok"] : reasons.length || trail ? ["Review", "warn"] : ["Good", "ok"];

  return (
    <section className="blink" aria-label="Blink">
      <div className="blink-stage">
        <div className="blink-bar" role="toolbar" aria-label="Playback">
          <button type="button" className="ft-tool" onClick={() => step(-1)} aria-label="Previous (←)">⏮</button>
          <button type="button" className="ft-tool on" onClick={togglePlay} aria-label={playing ? "Pause (space)" : "Play (space)"}>
            {playing ? "❚❚" : "▶"}
          </button>
          <button type="button" className="ft-tool" onClick={cycleFps} title="Frames per second">{fps} fps</button>
          <button type="button" className="ft-tool" onClick={() => step(1)} aria-label="Next (→)">⏭</button>
          <span className="ft-sep" aria-hidden="true" />
          <button type="button" className={`ft-tool ${lock ? "on" : ""}`} aria-pressed={lock} onClick={toggleLock} title="Keep the first frame's stretch so changes in sky and clouds show">
            Lock stretch
          </button>
          <span className="blink-count">{group(index + 1)} of {group(paths.length)}</span>
          <button type="button" className="ex-esc" onClick={closeBlink} aria-label="Exit blink (Esc)">
            <Kbd>esc</Kbd>
          </button>
        </div>
        {error ? <p className="stage-error">{error}</p> : <canvas ref={canvas} className="blink-canvas" style={frame?.bottomUp ? { transform: "scaleY(-1)" } : undefined} aria-label={path ? baseName(path) : ""} />}
        <div className="blink-hud mono">
          <span>{time(grade?.date_obs)}</span> <span className="muted">{path ? baseName(path) : ""}</span>
          {st && (
            <span>
              {" "}· stars <b>{st.stars}</b> · HFR <b>{st.hfr?.toFixed(1) ?? "—"}</b>
            </span>
          )}
          {trail && <div className="hud-flag">Satellite trail detected</div>}
        </div>
      </div>

      <aside className="blink-side" aria-label="This sub">
        <div className="rcard">
          <div className="rcard-head">
            <h2>Grade</h2>
            <span className={`ft-badge ${status[1]}`}>{status[0]}</span>
          </div>
          {st ? (
            <dl className="gradegrid">
              <div><dt>Stars</dt><dd>{st.stars}{median != null && <small> / {Math.round(median)} med</small>}</dd></div>
              <div><dt>HFR</dt><dd>{st.hfr?.toFixed(1) ?? "—"} px</dd></div>
              <div><dt>Background</dt><dd>{st.background.toFixed(3)}</dd></div>
              <div><dt>Eccentricity</dt><dd>{st.eccentricity?.toFixed(2) ?? "—"}</dd></div>
            </dl>
          ) : (
            <p className="muted small">{live === "loading" ? "Measuring…" : live === "error" ? "Could not measure." : "—"}</p>
          )}
          {reasons.length > 0 && <p className="small reasons-line">{reasons.map((r) => r.replace("_", " ")).join(", ")}</p>}
        </div>
        <div className="blink-actions">
          <Button variant={m === "keep" ? "primary" : "secondary"} onClick={() => mark("keep")}>Keep</Button>
          <button type="button" className={`ft-btn reject ${m === "reject" ? "on" : ""}`} onClick={() => mark("reject")}>Reject (R)</button>
        </div>
        {trail && <div className="rcard hint">Sigma-clip stacking usually removes single trails. Keeping it adds {grade?.exposure_s ?? "its"} s.</div>}
        {reasons.includes("clouds") && <div className="rcard hint">Few stars for this session: thin cloud or haze.</div>}
        {reasons.includes("low_altitude") && <div className="rcard hint">Target low in the sky ({grade?.altitude?.toFixed(0)}°): fewer stars through more air.</div>}
        <div className="blink-foot">
          <span className="muted small">{rejects} marked reject</span>
          {rejects > 0 &&
            (confirm ? (
              <span className="confirm">
                <Button variant="ghost" onClick={() => setConfirm(false)}>Cancel</Button>
                <Button variant="primary" onClick={() => (setConfirm(false), void moveMarked())}>Move {rejects}</Button>
              </span>
            ) : (
              <Button variant="secondary" onClick={() => setConfirm(true)}>Move to _rejected/…</Button>
            ))}
          <Button variant="ghost" onClick={() => path && (closeBlink(), void openFile(path, false))}>Open in viewer</Button>
        </div>
      </aside>

      <div className="blink-strip" aria-label="Filmstrip">
        {shown.map((p, k) => (
          <Thumb key={p} path={p} on={lo + k === index} rejected={marks[p] === "reject"} onClick={() => jump(lo + k)} />
        ))}
        {paths.length > shown.length && <span className="strip-more">+{group(paths.length - shown.length)}</span>}
      </div>
    </section>
  );
}
