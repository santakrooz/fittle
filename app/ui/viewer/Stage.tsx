import { useEffect, useRef } from "react";
import { app, getBackend, hover, onViewCommand } from "../state/app";
import { shaderStretch } from "../state/stretch";
import { Renderer, type ViewState } from "./renderer";
import { fitView, screenToDisplay, visibleRect, zoomAt } from "./transform";

const CHANNEL = { rgb: 0, r: 1, g: 2, b: 3 } as const;
/** Largest detail patch requested, in pixels. */
const MAX_DETAIL = 4_200_000;

export function Stage() {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current!;
    let r: Renderer;
    try {
      r = new Renderer(canvas);
    } catch (e) {
      app.set({ error: String(e) });
      return;
    }
    let fitted = true;
    let detailTimer = 0;
    let detailSeq = 0;
    let readoutBusy = false;
    let pending: { x: number; y: number } | null = null;

    const size = () => ({ width: canvas.width, height: canvas.height });
    const img = () => {
      const d = app.get().display;
      return d ? { width: d.width, height: d.height } : null;
    };
    const flip = () => app.get().opened?.info.fields.row_order?.value.toUpperCase() === "BOTTOM-UP";
    const sourceStep = () => app.get().display?.source_step ?? 1;

    const publishZoom = () => hover.set({ zoom: r.view.scale / sourceStep() });
    const setView = (v: ViewState, isFit = false) => {
      r.view = v;
      fitted = isFit;
      publishZoom();
      r.request();
      scheduleDetail();
    };
    const fit = () => {
      const i = img();
      if (i) setView(fitView(i, size()), true);
    };

    // Full-resolution patch for the visible area once zoomed past the preview.
    const scheduleDetail = () => {
      clearTimeout(detailTimer);
      detailTimer = window.setTimeout(async () => {
        const s = app.get();
        if (!s.display || !s.preview) return;
        const previewScale = s.display.width / s.preview.width;
        if (r.view.scale * previewScale <= 1.2) {
          r.setDetail(null);
          return;
        }
        const rect = visibleRect(r.view, size(), s.display, flip());
        if (rect.w < 2 || rect.h < 2) return;
        let stepPx = Math.max(1, Math.floor(1 / r.view.scale));
        while ((rect.w / stepPx) * (rect.h / stepPx) > MAX_DETAIL) stepPx++;
        const seq = ++detailSeq;
        const px = await getBackend().region(s.mode, rect.x, rect.y, rect.w, rect.h, stepPx);
        if (seq !== detailSeq) return;
        r.setDetail(px, rect.x, rect.y, px.width * stepPx, px.height * stepPx);
      }, 160);
    };

    const applyStretch = () => {
      const s = app.get();
      if (!s.display) return;
      r.stretch = shaderStretch(s.display, s.stretch);
      r.clipping = s.stretch.clipping;
      r.channel = CHANNEL[s.stretch.channel];
      r.request();
    };

    // React to state without re-rendering: new pixels, stretch changes.
    let lastPreview = app.get().preview;
    let lastStretch = app.get().stretch;
    const onState = () => {
      const s = app.get();
      if (s.preview !== lastPreview) {
        lastPreview = s.preview;
        if (s.preview && s.display) {
          const keep = r.hasImage() && !fitted;
          r.setImage(s.preview, s.display.width, s.display.height, flip());
          applyStretch();
          if (!keep) fit();
          else scheduleDetail();
        } else {
          r.dispose();
          r.request();
        }
      }
      if (s.stretch !== lastStretch) {
        lastStretch = s.stretch;
        applyStretch();
      }
    };
    const unsub = app.subscribe(onState);
    onState();

    const ro = new ResizeObserver(() => {
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(canvas.clientWidth * dpr);
      canvas.height = Math.round(canvas.clientHeight * dpr);
      if (fitted) fit();
      else r.request();
    });
    ro.observe(canvas);

    const unView = onViewCommand((c) => {
      const centre = [canvas.width / 2, canvas.height / 2] as const;
      if (c === "fit") fit();
      else if (c === "one") setView({ ...r.view, scale: sourceStep() });
      else setView(zoomAt(r.view, c === "in" ? 1.5 : 1 / 1.5, centre[0], centre[1], size()));
    });

    // ---- pointer ----------------------------------------------------------
    const dpr = () => window.devicePixelRatio || 1;
    const local = (e: PointerEvent | WheelEvent | MouseEvent) => {
      const b = canvas.getBoundingClientRect();
      return [(e.clientX - b.left) * dpr(), (e.clientY - b.top) * dpr()] as const;
    };
    let drag: { x: number; y: number; cx: number; cy: number } | null = null;

    const readout = async () => {
      if (readoutBusy || !pending) return;
      const p = pending;
      pending = null;
      readoutBusy = true;
      try {
        hover.set({ readout: await getBackend().readout(p.x, p.y) });
      } finally {
        readoutBusy = false;
        if (pending) readout();
      }
    };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const [sx, sy] = local(e);
      setView(zoomAt(r.view, Math.exp(-e.deltaY * 0.0015), sx, sy, size()));
    };
    const onDown = (e: PointerEvent) => {
      canvas.setPointerCapture(e.pointerId);
      const [x, y] = local(e);
      drag = { x, y, cx: r.view.cx, cy: r.view.cy };
      canvas.style.cursor = "grabbing";
    };
    const onMove = (e: PointerEvent) => {
      const [sx, sy] = local(e);
      if (drag) {
        setView({ scale: r.view.scale, cx: drag.cx - (sx - drag.x) / r.view.scale, cy: drag.cy - (sy - drag.y) / r.view.scale });
        return;
      }
      const i = img();
      if (!i) return;
      const p = screenToDisplay(r.view, sx, sy, size(), i, flip());
      if (!p) {
        hover.set({ readout: null });
        return;
      }
      const st = sourceStep();
      pending = { x: p.x * st, y: p.y * st };
      readout();
    };
    const onUp = (e: PointerEvent) => {
      drag = null;
      canvas.style.cursor = "";
      canvas.releasePointerCapture(e.pointerId);
    };
    const onLeave = () => hover.set({ readout: null });
    const onDbl = (e: MouseEvent) => {
      const [sx, sy] = local(e);
      if (fitted) setView(zoomAt(r.view, sourceStep() / r.view.scale, sx, sy, size()));
      else fit();
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    canvas.addEventListener("pointerdown", onDown);
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerup", onUp);
    canvas.addEventListener("pointerleave", onLeave);
    canvas.addEventListener("dblclick", onDbl);

    return () => {
      unsub();
      unView();
      ro.disconnect();
      clearTimeout(detailTimer);
      canvas.removeEventListener("wheel", onWheel);
      canvas.removeEventListener("pointerdown", onDown);
      canvas.removeEventListener("pointermove", onMove);
      canvas.removeEventListener("pointerup", onUp);
      canvas.removeEventListener("pointerleave", onLeave);
      canvas.removeEventListener("dblclick", onDbl);
      r.dispose();
    };
  }, []);

  return <canvas ref={canvasRef} className="stage-canvas" aria-label="Image, stretched for display" />;
}
