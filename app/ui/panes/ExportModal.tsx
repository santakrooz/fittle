import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { Bytes, ExportPlan, Exported } from "../backend/types";
import { Button, Kbd, Segmented } from "../ds";
import { baseName, group } from "../format";
import { app, getBackend } from "../state/app";
import { closeExport, exporter, setChoices, specOf, type FormatKind, type StretchChoice } from "../state/exporter";

const FORMATS: { value: FormatKind; label: string }[] = [
  { value: "tiff", label: "TIFF" },
  { value: "png", label: "PNG" },
  { value: "jpeg", label: "JPEG" },
  { value: "webp", label: "WebP" },
  { value: "avif", label: "AVIF" },
  { value: "fits", label: "FITS" },
];

const STRETCHES: { value: StretchChoice; label: string }[] = [
  { value: "none", label: "None (linear)" },
  { value: "view", label: "Current view" },
  { value: "auto", label: "Auto STF" },
];

const EDGES = [null, 4096, 3000, 2048, 1600, 1080];
const QUALITIES = [100, 95, 90, 85, 80, 70];
const PREVIEW_EDGE = 720;

export function bytes(n: number) {
  if (n < 1e3) return `${n} B`;
  if (n < 1e6) return `${Math.round(n / 1e3)} kB`;
  return `${(n / 1e6).toFixed(1)} MB`;
}

function draw(canvas: HTMLCanvasElement, px: Bytes) {
  canvas.width = px.width;
  canvas.height = px.height;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const img = ctx.createImageData(px.width, px.height);
  const n = px.width * px.height;
  for (let i = 0; i < n; i++) {
    const [r, g, b] = px.channels === 3 ? [px.data[i * 3], px.data[i * 3 + 1], px.data[i * 3 + 2]] : [px.data[i], px.data[i], px.data[i]];
    img.data[i * 4] = r;
    img.data[i * 4 + 1] = g;
    img.data[i * 4 + 2] = b;
    img.data[i * 4 + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
}

export function ExportModal() {
  const open = exporter.use((s) => s.open);
  const c = exporter.use((s) => s.choices);
  const opened = app.use((s) => s.opened);
  // The viewer's stretch feeds "Current view"; re-preview when it changes.
  const viewStretch = app.use((s) => s.stretch);
  const display = app.use((s) => s.display);
  const [plan, setPlan] = useState<ExportPlan | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<Exported | null>(null);
  const [error, setError] = useState<string | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      setDone(null);
      setError(null);
      setTimeout(() => dialog.current?.focus(), 0);
    }
  }, [open]);

  // Plan (cheap) on every change; preview debounced, latest request wins.
  useEffect(() => {
    if (!open || !opened?.image) return;
    const spec = specOf(c);
    let live = true;
    getBackend()
      .exportPlan(spec, c.template)
      .then((p) => live && setPlan(p))
      .catch((e) => live && setError(String(e)));
    const t = setTimeout(() => {
      getBackend()
        .exportPreview(spec, PREVIEW_EDGE)
        .then((px) => {
          if (!live || !canvas.current) return;
          draw(canvas.current, px);
          setPreviewError(null);
        })
        .catch((e) => live && setPreviewError(e instanceof Error ? e.message : String(e)));
    }, 120);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [open, c, opened, viewStretch, display]);

  if (!open || !opened?.image) return null;
  const image = opened.image;
  const isFits = c.format === "fits";
  const lossy = c.format === "jpeg" || c.format === "avif";

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      setDone(await getBackend().exportImage(specOf(c), c.template, c.dir));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const chooseDir = async () => {
    const d = await getBackend().pickFolder();
    if (d) setChoices({ dir: d });
  };
  const onKey = (e: ReactKeyboardEvent) => {
    if (e.key === "Escape") (e.stopPropagation(), closeExport());
    else if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && !busy) void run();
  };
  const depth =
    c.format === "png" ? (
      <Segmented label="Bit depth" value={String(c.pngBits) as "8" | "16"} onChange={(v) => setChoices({ pngBits: Number(v) as 8 | 16 })}
        options={[{ value: "8", label: "8-bit" }, { value: "16", label: "16-bit" }]} />
    ) : c.format === "tiff" ? (
      <Segmented label="Bit depth" value={String(c.tiffBits) as "8" | "16" | "32"} onChange={(v) => setChoices({ tiffBits: Number(v) as 8 | 16 | 32 })}
        options={[{ value: "8", label: "8-bit" }, { value: "16", label: "16-bit" }, { value: "32", label: "32-bit float" }]} />
    ) : null;
  const ext = plan?.file_name.split(".").pop();

  return (
    <div className="scrim center" onMouseDown={(e) => e.target === e.currentTarget && closeExport()}>
      <div className="export" role="dialog" aria-modal="true" aria-label="Export image" tabIndex={-1} ref={dialog} onKeyDown={onKey}>
        <div className="ex-preview">
          <canvas ref={canvas} aria-label="Export preview" hidden={!!previewError} />
          {previewError && <p className="muted small">{previewError}</p>}
          {plan && (
            <p className="ex-dims mono">
              {group(plan.width)} × {group(plan.height)}
              {plan.channels === 3 ? " RGB" : " mono"} · ≈ {bytes(plan.estimate_bytes)}
            </p>
          )}
        </div>
        <div className="ex-form">
          <div className="ex-head">
            <h2>Export image</h2>
            <button type="button" className="ex-esc" onClick={closeExport} aria-label="Close">
              <Kbd>esc</Kbd>
            </button>
          </div>

          <div className="ex-group">
            <span className="ex-label">Format</span>
            <div className="ex-tiles" role="radiogroup" aria-label="Format">
              {FORMATS.map((f) => (
                <button key={f.value} type="button" role="radio" aria-checked={c.format === f.value} className="ex-tile" onClick={() => setChoices({ format: f.value })}>
                  {f.label}
                </button>
              ))}
            </div>
            {depth && <div className="ex-sub">{depth}</div>}
            {c.format === "webp" && <p className="ex-note">Lossless WebP.</p>}
          </div>

          <div className="ex-group">
            <span className="ex-label">Stretch</span>
            <Segmented label="Stretch" value={c.stretch} onChange={(v) => setChoices({ stretch: v })} options={STRETCHES} />
            {c.stretch !== "none" && (
              <p className="ex-note">
                A display stretch, baked in and labelled as such{isFits ? " in HISTORY" : ""}.
                {isFits && " Choose None to keep linear data."}
              </p>
            )}
          </div>

          <div className="ex-row">
            <label className="ex-group">
              <span className="ex-label">Long edge</span>
              <select className="ex-input mono" value={c.longEdge ?? ""} onChange={(e) => setChoices({ longEdge: e.target.value ? Number(e.target.value) : null })}>
                {EDGES.map((e) => (
                  <option key={e ?? 0} value={e ?? ""}>
                    {e ? `${e} px` : `Original (${Math.max(image.width, image.height)} px)`}
                  </option>
                ))}
              </select>
            </label>
            <label className="ex-group">
              <span className="ex-label">Quality</span>
              <select className="ex-input mono" value={lossy ? c.quality : ""} disabled={!lossy} onChange={(e) => setChoices({ quality: Number(e.target.value) })}>
                {!lossy && <option value="">Lossless</option>}
                {QUALITIES.map((q) => (
                  <option key={q} value={q}>
                    {q}
                  </option>
                ))}
              </select>
            </label>
          </div>

          <label className="ex-group">
            <span className="ex-label">File name</span>
            <span className="ex-template">
              <input className="ex-input mono" value={c.template} spellCheck={false} onChange={(e) => setChoices({ template: e.target.value })} aria-describedby="ex-name" />
              <span className="ex-ext mono">.{ext}</span>
            </span>
            <span id="ex-name" className="ex-resolved mono">
              → {plan?.file_name ?? "…"}
              {plan && plan.missing.length > 0 && <span className="ex-warn"> · no {plan.missing.map((m) => `{${m}}`).join(", ")} in this file</span>}
            </span>
          </label>

          <div className="ex-checks">
            <label>
              <input type="checkbox" checked={c.metadata && !isFits} disabled={isFits} onChange={(e) => setChoices({ metadata: e.target.checked })} />
              {isFits ? "FITS keeps its header (plate solution updated)" : c.format === "avif" ? "Embed acquisition summary (EXIF)" : "Embed acquisition summary in XMP"}
            </label>
            <label>
              <input type="checkbox" checked={c.private} onChange={(e) => setChoices({ private: e.target.checked })} />
              Strip site coordinates and serial numbers
            </label>
            <label>
              <input type="checkbox" checked={c.card && !isFits} disabled={isFits} onChange={(e) => setChoices({ card: e.target.checked })} />
              Add caption strip (share card)
            </label>
            {image.can_debayer && (
              <label>
                <input type="checkbox" checked={c.debayer} onChange={(e) => setChoices({ debayer: e.target.checked })} />
                Debayer to colour (bilinear, full size)
              </label>
            )}
          </div>

          <div className="ex-dest">
            <span className="muted small">
              Save to <span className="mono">{c.dir ? baseName(c.dir) : "the source folder"}</span>
            </span>
            <Button variant="ghost" onClick={chooseDir}>
              Change…
            </Button>
          </div>

          {plan?.problem && <p className="ex-error">{plan.problem}</p>}
          {error && <p className="ex-error">{error}</p>}
          {done && (
            <p className="ex-done">
              Saved <span className="mono">{baseName(done.path)}</span> · {bytes(done.bytes)}
              {done.wcs && " · plate solution kept"}
            </p>
          )}

          <div className="ex-actions">
            <Button onClick={closeExport}>{done ? "Close" : "Cancel"}</Button>
            <Button variant="primary" onClick={run} disabled={busy || !!plan?.problem}>
              {busy ? "Exporting…" : done ? "Export again" : "Export"}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
