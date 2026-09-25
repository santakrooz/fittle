import { describe, expect, it } from "vitest";
import { decDms, duration, raHms, shortName } from "./format";

describe("format", () => {
  it("matches the CLI", () => {
    expect(raHms(314.295)).toBe("20h 57m 10.8s");
    expect(decDms(31.235278)).toBe("+31° 14′ 07″");
    expect(decDms(-5.391111)).toBe("−05° 23′ 28″");
    expect(duration(15900)).toBe("4 h 25 m");
    expect(duration(20)).toBe("20 s");
  });
  it("shortens names from the front", () => {
    const s = shortName("Light_NGC 6995_20.0s_LP_20260924-213412.fit");
    expect(s).toHaveLength(22);
    expect(s.startsWith("…") && s.endsWith("0924-213412.fit")).toBe(true);
    expect(shortName("short.fit")).toBe("short.fit");
  });
});
