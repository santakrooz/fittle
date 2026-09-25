// Screen ↔ image coordinates. "Display" pixels are the pixels of the mode
// being shown (half-size for superpixel debayer); "visual" y is display y
// after the row-order flip; canvas units are device pixels.
import type { ViewState } from "./renderer";

export type Size = { width: number; height: number };

/** Fit the whole image with a small margin. */
export function fitView(img: Size, canvas: Size, margin = 0.94): ViewState {
  const scale = Math.min(canvas.width / img.width, canvas.height / img.height) * margin;
  return { scale, cx: img.width / 2, cy: img.height / 2 };
}

/** Zoom by `factor` keeping the image point under (sx, sy) fixed. */
export function zoomAt(v: ViewState, factor: number, sx: number, sy: number, canvas: Size, limits = [0.02, 64]): ViewState {
  const scale = Math.min(limits[1], Math.max(limits[0], v.scale * factor));
  const ix = (sx - canvas.width / 2) / v.scale + v.cx;
  const iy = (sy - canvas.height / 2) / v.scale + v.cy;
  return { scale, cx: ix - (sx - canvas.width / 2) / scale, cy: iy - (sy - canvas.height / 2) / scale };
}

/** Screen (device px) → display pixel in stored order, or null if outside. */
export function screenToDisplay(v: ViewState, sx: number, sy: number, canvas: Size, img: Size, flip: boolean) {
  const x = (sx - canvas.width / 2) / v.scale + v.cx;
  const vy = (sy - canvas.height / 2) / v.scale + v.cy;
  const y = flip ? img.height - vy : vy;
  if (x < 0 || y < 0 || x >= img.width || y >= img.height) return null;
  return { x: Math.floor(x), y: Math.floor(y) };
}

/** Visible part of the image as a stored-order display rect, clamped. */
export function visibleRect(v: ViewState, canvas: Size, img: Size, flip: boolean) {
  const x0 = Math.max(0, Math.floor(v.cx - canvas.width / 2 / v.scale));
  const x1 = Math.min(img.width, Math.ceil(v.cx + canvas.width / 2 / v.scale));
  let y0 = Math.max(0, Math.floor(v.cy - canvas.height / 2 / v.scale));
  let y1 = Math.min(img.height, Math.ceil(v.cy + canvas.height / 2 / v.scale));
  if (flip) [y0, y1] = [img.height - y1, img.height - y0];
  return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
}
