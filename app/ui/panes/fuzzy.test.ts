import { describe, expect, it } from "vitest";
import { fuzzy } from "./fuzzy";

describe("fuzzy", () => {
  it("prefers contiguous word-start matches", () => {
    expect(fuzzy("exp", "Export image")!.score).toBeGreaterThan(fuzzy("exp", "Show exposure")!.score);
  });
  it("matches subsequences", () => {
    expect(fuzzy("atf", "Auto STF")?.hits).toEqual([0, 2, 7]);
  });
  it("rejects missing letters", () => expect(fuzzy("xyz", "Auto STF")).toBeNull());
});
