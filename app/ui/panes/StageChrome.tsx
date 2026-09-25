import { Separator, ToolButton, Toolbar } from "../ds";
import { adu, bitDepth, decDms, num, raHms } from "../format";
import { app, hover, setMode, setStretch, view } from "../state/app";
import { openExport } from "../state/exporter";
import type { StretchKind } from "../state/stretch";

const KINDS: { value: StretchKind; label: string; key: string }[] = [
  { value: "auto", label: "Auto STF", key: "A" },
  { value: "linear", label: "Linear", key: "L" },
  { value: "asinh", label: "Asinh", key: "H" },
];

export function StageToolbar() {
  const stretch = app.use((s) => s.stretch);
  const image = app.use((s) => s.opened?.image);
  const mode = app.use((s) => s.mode);
  const channels = app.use((s) => s.display?.channels ?? 1);
  if (!image) return null;
  return (
    <div className="stage-toolbar">
      <Toolbar label="View">
        {KINDS.map((k) => (
          <ToolButton key={k.value} on={stretch.kind === k.value} label={`${k.label} (${k.key})`} onClick={() => setStretch({ kind: k.value })}>
            {k.label}
          </ToolButton>
        ))}
        {stretch.kind === "manual" && (
          <ToolButton on label="Manual stretch from the histogram">
            Manual
          </ToolButton>
        )}
        <Separator />
        <ToolButton icon="zoomOut" label="Zoom out (−)" onClick={() => view("out")} />
        <ToolButton label="Fit (F)" onClick={() => view("fit")}>
          Fit
        </ToolButton>
        <ToolButton icon="zoomIn" label="Zoom in (+)" onClick={() => view("in")} />
        <ToolButton label="Actual pixels (1)" onClick={() => view("one")}>
          1:1
        </ToolButton>
        <Separator />
        {image.can_debayer && (
          <ToolButton
            on={mode === "debayer"}
            label={mode === "debayer" ? "Showing debayered colour (D for raw CFA)" : "Showing raw CFA (D to debayer)"}
            onClick={() => setMode(mode === "debayer" ? "raw" : "debayer")}
          >
            {mode === "debayer" ? "RGB" : "CFA"}
          </ToolButton>
        )}
        <ToolButton on={stretch.clipping} label="Clipping overlay (C)" onClick={() => setStretch({ clipping: !stretch.clipping })}>
          Clipping
        </ToolButton>
        {channels === 3 &&
          (["r", "g", "b"] as const).map((c) => (
            <ToolButton
              key={c}
              on={stretch.channel === c}
              label={`Show only ${c.toUpperCase()}`}
              onClick={() => setStretch({ channel: stretch.channel === c ? "rgb" : c })}
            >
              {c.toUpperCase()}
            </ToolButton>
          ))}
        <Separator />
        <ToolButton icon="export" label="Export image (⌘E)" onClick={() => openExport()} />
      </Toolbar>
    </div>
  );
}

function Readout() {
  const r = hover.use((s) => s.readout);
  const planes = app.use((s) => s.opened?.image?.planes ?? 1);
  if (!r) return <div className="hud-box muted">Move over the image to read pixels</div>;
  const values =
    planes === 3
      ? (["R", "G", "B"] as const).map((c, i) => (
          <span key={c}>
            {c} <b>{adu(r.raw[i])}</b>
          </span>
        ))
      : [
          <span key="v">
            ADU <b>{adu(r.raw[0])}</b>
          </span>,
        ];
  return (
    <div className="hud-box">
      <span>
        x <b>{r.x}</b>
      </span>
      <span>
        y <b>{r.y}</b>
      </span>
      {values}
      {r.ra != null && r.dec != null && (
        <>
          <span>
            RA <b>{raHms(r.ra)}</b>
          </span>
          <span>
            Dec <b>{decDms(r.dec)}</b>
          </span>
        </>
      )}
    </div>
  );
}

export function Hud() {
  const image = app.use((s) => s.opened?.image);
  const bayer = app.use((s) => s.opened?.info.fields.bayer?.value);
  const mode = app.use((s) => s.mode);
  const zoom = hover.use((s) => s.zoom);
  const bitpix = app.use((s) => s.opened?.info.image?.bitpix);
  if (!image) return null;
  const colour = image.planes === 3 ? "RGB" : bayer ? (mode === "debayer" ? `${bayer} → RGB` : bayer) : "Mono";
  return (
    <div className="hud">
      <Readout />
      <div className="hud-box">
        <span>
          <b>
            {image.width} × {image.height}
          </b>
        </span>
        <span>{bitDepth(bitpix)}</span>
        <span>{colour}</span>
        <span>{num(zoom * 100, zoom < 0.1 ? 1 : 0)}%</span>
        <span className="muted">display stretch only</span>
      </div>
    </div>
  );
}
