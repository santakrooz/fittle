import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { Display, Stf } from "../backend/types";
import { Button, Card, Segmented } from "../ds";
import { app, setStretch } from "../state/app";
import { openExport } from "../state/exporter";
import { applyStf, autoStf, shaderStretch, type Stretch } from "../state/stretch";

const CH_COLORS = ["--nebula-rose", "--ok", "--nebula-teal"];
const MONO = ["--text-muted"];

function css(name: string) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
}

/** The curve and handles shown: the first channel's effective stretch. */
function currentStf(d: Display, s: Stretch): Stf {
  const sh = shaderStretch(d, s);
  return { shadows: sh.shadows[0], midtones: sh.midtones[0], highlights: sh.highlights[0] };
}

/** Upper end of the plotted range: fits linear data (median + 40 MAD, ≥ the white point). */
export function fitRange(d: Display, s: Stretch): number {
  const hi = Math.max(...d.stats.map((c) => c.median + 40 * c.mad), currentStf(d, s).highlights < 1 ? currentStf(d, s).highlights : 0);
  return Math.min(1, Math.max(0.01, hi * 1.15));
}

function Plot({ d, stretch, xMax }: { d: Display; stretch: Stretch; xMax: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = ref.current!;
    const dpr = window.devicePixelRatio || 1;
    const W = (c.width = Math.round(c.clientWidth * dpr));
    const H = (c.height = Math.round(c.clientHeight * dpr));
    const g = c.getContext("2d")!;
    g.clearRect(0, 0, W, H);
    g.strokeStyle = css("--line");
    g.lineWidth = 1;
    for (let i = 1; i < 4; i++) {
      g.beginPath();
      g.moveTo(0, (H * i) / 4);
      g.lineTo(W, (H * i) / 4);
      g.stroke();
    }
    const colors = d.channels === 3 ? CH_COLORS : MONO;
    // Re-bin the visible part of each histogram to about one bin per 2 px.
    const cols = Math.max(32, Math.round(W / (2 * dpr)));
    const binned = d.stats.map((s) => {
      const n = s.histogram.length;
      const out = new Float64Array(cols);
      const last = Math.min(n, Math.ceil(xMax * n));
      for (let i = 1; i < last - (xMax >= 1 ? 1 : 0); i++) out[Math.min(cols - 1, Math.floor(((i / n) / xMax) * cols))] += s.histogram[i];
      return out;
    });
    const max = Math.max(1, ...binned.flatMap((b) => Array.from(b)));
    const y = (v: number) => H - (Math.log1p(v) / Math.log1p(max)) * (H - 4 * dpr);
    binned.forEach((b, ci) => {
      const color = css(colors[ci % colors.length]);
      g.beginPath();
      g.moveTo(0, H);
      b.forEach((v, i) => g.lineTo(((i + 0.5) / cols) * W, y(v)));
      g.lineTo(W, H);
      g.closePath();
      g.globalAlpha = 0.28;
      g.fillStyle = color;
      g.fill();
      g.globalAlpha = 1;
      g.strokeStyle = color;
      g.lineWidth = 1.25 * dpr;
      g.stroke();
    });
    // The transfer curve.
    const stf = currentStf(d, stretch);
    g.setLineDash([4 * dpr, 4 * dpr]);
    g.strokeStyle = css("--accent");
    g.lineWidth = 1.5 * dpr;
    g.beginPath();
    for (let i = 0; i <= 200; i++) {
      const x = (i / 200) * xMax;
      const v = stretch.kind === "asinh" ? shaderAsinh(d, stretch, x) : applyStf(stf, x);
      const px = (x / xMax) * W;
      const py = H - v * (H - 2 * dpr);
      if (i) g.lineTo(px, py);
      else g.moveTo(px, py);
    }
    g.stroke();
    g.setLineDash([]);
  }, [d, stretch, xMax]);
  return (
    <canvas
      ref={ref}
      className="hist-canvas"
      aria-label="Histogram with the display curve"
      title="Double-click to reset to Auto STF"
      onDoubleClick={reset}
    />
  );
}

function shaderAsinh(d: Display, s: Stretch, v: number) {
  const sh = shaderStretch(d, s);
  const x = Math.min(1, Math.max(0, (v - sh.shadows[0]) / (sh.highlights[0] - sh.shadows[0])));
  return Math.asinh(sh.asinh * x) / Math.asinh(sh.asinh);
}

const reset = () => setStretch({ kind: "auto" });

/** Black / mid / white handles. Dragging switches to a manual stretch. */
function Handles({ d, stretch, xMax }: { d: Display; stretch: Stretch; xMax: number }) {
  const track = useRef<HTMLDivElement>(null);
  const stf = currentStf(d, stretch);
  // Midtone handle sits where the curve crosses 0.5.
  const midX = stf.shadows + (stf.highlights - stf.shadows) * stf.midtones;

  const drag = (which: "shadows" | "mid" | "highlights") => (e: ReactPointerEvent) => {
    const el = track.current!;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const base = stretch.kind === "manual" ? stretch.manual : autoStf(d, stretch.linked);
    const move = (ev: PointerEvent) => {
      const b = el.getBoundingClientRect();
      const x = Math.min(1, Math.max(0, ((ev.clientX - b.left) / b.width) * xMax));
      const next = base.map((s) => {
        const t = { ...s };
        if (which === "shadows") t.shadows = Math.min(x, t.highlights - 0.001);
        else if (which === "highlights") t.highlights = Math.max(x, t.shadows + 0.001);
        else t.midtones = Math.min(0.999, Math.max(0.001, (x - t.shadows) / (t.highlights - t.shadows)));
        return t;
      });
      setStretch({ kind: "manual", manual: next });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <>
      <div ref={track} className="slider">
        <div className="track" />
        <button type="button" className="knob" style={{ left: `${Math.min(100, (stf.shadows / xMax) * 100)}%` }} aria-label="Black point (double-click to reset)" title="Drag to set the black point · double-click to reset" onPointerDown={drag("shadows")} onDoubleClick={reset} />
        <button type="button" className="knob a" style={{ left: `${Math.min(100, (midX / xMax) * 100)}%` }} aria-label="Midtones (double-click to reset)" title="Drag to set the midtones · double-click to reset" onPointerDown={drag("mid")} onDoubleClick={reset} />
        <button type="button" className="knob" style={{ left: `${Math.min(100, (stf.highlights / xMax) * 100)}%` }} aria-label="White point (double-click to reset)" title="Drag to set the white point · double-click to reset" onPointerDown={drag("highlights")} onDoubleClick={reset} />
      </div>
      <div className="ft-fields three">
        <Val label="Black" v={stf.shadows} />
        <Val label="Mid" v={stf.midtones} />
        <Val label="White" v={stf.highlights} />
      </div>
    </>
  );
}

const Val = ({ label, v }: { label: string; v: number }) => (
  <div>
    <div className="ft-label">{label}</div>
    <div className="ft-value mono">{v.toFixed(4)}</div>
  </div>
);

const pct = (v: number) => `${(v * 100).toFixed(2)}%`;

export function HistogramTab() {
  const d = app.use((s) => s.display);
  const stretch = app.use((s) => s.stretch);
  const [range, setRange] = useState<"fit" | "full">("fit");
  if (!d) return <p className="inspector-empty">No image loaded.</p>;
  const xMax = range === "full" ? 1 : fitRange(d, stretch);
  const tick = (v: number) => (xMax >= 1 ? String(v) : v.toFixed(xMax < 0.1 ? 3 : 2));
  const names = d.channels === 3 ? ["R", "G", "B"] : ["Value"];
  const rows: [string, (i: number) => string, (i: number) => boolean][] = [
    ["Median", (i) => d.stats[i].median.toFixed(4), () => false],
    ["MAD", (i) => d.stats[i].mad.toFixed(4), () => false],
    ["Clipped low", (i) => pct(d.stats[i].clipped_low), (i) => d.stats[i].clipped_low > 0.001],
    ["Saturated", (i) => pct(d.stats[i].saturated), (i) => d.stats[i].saturated > 0.0005],
  ];
  return (
    <div className="inspector-body">
      {d.channels === 3 && (
        <Segmented
          label="Channel link"
          value={stretch.linked ? "linked" : "unlinked"}
          onChange={(v) => setStretch({ linked: v === "linked", ...(stretch.kind === "manual" ? { kind: "auto" } : {}) })}
          options={[
            { value: "linked", label: "Linked" },
            { value: "unlinked", label: "Unlinked" },
          ]}
        />
      )}
      <Card
        title="Histogram"
        badge={
          <div className="ft-row hist-head">
            {stretch.kind !== "auto" && (
              <button type="button" className="ft-chip" onClick={reset} title="Back to the automatic stretch (A)">
                ↺ Reset
              </button>
            )}
            <Segmented
              label="Histogram range"
              value={range}
              onChange={setRange}
              options={[
                { value: "fit", label: "Fit data" },
                { value: "full", label: "0–1" },
              ]}
            />
          </div>
        }
      >
        <Plot d={d} stretch={stretch} xMax={xMax} />
        <div className="hist-axis">
          <span>0</span>
          <span>{tick(xMax / 2)}</span>
          <span>{tick(xMax)}</span>
        </div>
        <Handles d={d} stretch={stretch} xMax={xMax} />
      </Card>
      <Card title="Statistics" badge={<span className="ft-label">Linear, 0–1</span>}>
        <table className="gtable">
          <thead>
            <tr>
              <th />
              {names.map((n) => (
                <th key={n} className="num">
                  {n}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map(([label, val, bad]) => (
              <tr key={label}>
                <td>{label}</td>
                {names.map((n, i) => (
                  <td key={n} className={`num mono ${bad(i) ? "bad" : ""}`}>
                    {val(i)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
      <div className="ft-row">
        <Button onClick={reset} disabled={stretch.kind === "auto"}>
          Reset to Auto STF
        </Button>
        <Button variant="ghost" onClick={() => setStretch({ clipping: !stretch.clipping })}>
          {stretch.clipping ? "Hide clipping" : "Show clipping"}
        </Button>
        <Button variant="ghost" onClick={() => openExport(true)}>
          Export with this stretch
        </Button>
      </div>
      <p className="muted small">The file stays linear. This stretch is for viewing; exports label it as a display stretch.</p>
    </div>
  );
}
