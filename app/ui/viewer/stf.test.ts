import { describe, expect, it } from "vitest";
import { applyStf, mtf } from "./stf";

// Must agree with fittle-image `stretch::mtf` and the fragment shader.
describe("mtf", () => {
  it("is identity at m = 0.5", () => expect(mtf(0.5, 0.3)).toBe(0.3));
  it("maps x = m to 0.5", () => expect(mtf(0.2, 0.2)).toBeCloseTo(0.5, 6));
  it("fixes the endpoints", () => {
    expect(mtf(0.1, 0)).toBe(0);
    expect(mtf(0.1, 1)).toBe(1);
  });
  it("clips below shadows", () =>
    expect(applyStf({ shadows: 0.1, midtones: 0.3, highlights: 1 }, 0.05)).toBe(0));
});
