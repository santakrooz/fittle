import { describe, expect, it } from "vitest";
import { fitView, screenToDisplay, visibleRect, zoomAt } from "./transform";

const canvas = { width: 1000, height: 800 };
const img = { width: 2000, height: 1000 };

describe("view transform", () => {
  it("fits the image", () => {
    const v = fitView(img, canvas, 1);
    expect(v.scale).toBe(0.5);
    expect(screenToDisplay(v, 500, 400, canvas, img, false)).toEqual({ x: 1000, y: 500 });
  });

  it("keeps the point under the cursor when zooming", () => {
    const v = fitView(img, canvas, 1);
    const before = screenToDisplay(v, 100, 300, canvas, img, false);
    const z = zoomAt(v, 3, 100, 300, canvas);
    expect(screenToDisplay(z, 100, 300, canvas, img, false)).toEqual(before);
  });

  it("flips bottom-up rows", () => {
    const v = { scale: 1, cx: 1000, cy: 500 };
    expect(screenToDisplay(v, 500, 0, canvas, img, true)?.y).toBe(900);
    expect(visibleRect(v, canvas, img, true)).toEqual({ x: 500, y: 100, w: 1000, h: 800 });
  });

  it("returns null outside the image", () => {
    expect(screenToDisplay({ scale: 1, cx: 0, cy: 0 }, 0, 0, canvas, img, false)).toBeNull();
  });
});
