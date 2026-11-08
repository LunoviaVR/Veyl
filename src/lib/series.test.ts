import { describe, expect, it } from "vitest";
import { mergeSeries, seriesStats } from "./series";
import type { Series } from "./types";

const s = (key: string, t: number[], avg: (number | null)[]): Series => ({ key, unit: "%", bucketMs: 5000, source: "raw", t, avg, min: avg, max: avg });

describe("mergeSeries", () => {
  it("uses undefined (connected) for timestamps another series owns", () => {
    const m = mergeSeries([s("a", [0, 5000, 10000], [1, 2, 3]), s("b", [0, 10000], [7, 9])]);
    expect(m.data[0]).toEqual([0, 5, 10]);
    expect(m.data[2]).toEqual([7, undefined, 9]);
    expect(m.gaps).toEqual([]);
  });

  it("keeps explicit gap markers as null and reports the gap span", () => {
    const m = mergeSeries([s("a", [0, 5000, 60000], [1, null, 2]), s("b", [0, 5000, 60000], [3, null, 4])]);
    expect(m.data[1]).toEqual([1, null, 2]);
    expect(m.gaps).toEqual([[0, 60]]);
  });

  it("a null in one series only is still drawn as that series' gap", () => {
    const m = mergeSeries([s("a", [0, 5000, 10000], [1, null, 2]), s("b", [0, 5000, 10000], [3, 4, 5])]);
    expect(m.data[1]).toEqual([1, null, 2]);
    expect(m.gaps).toEqual([]);
  });

  it("handles empty input", () => {
    expect(mergeSeries([]).data).toEqual([[]]);
  });
});

describe("seriesStats", () => {
  it("ignores gaps", () => {
    expect(seriesStats(s("a", [0, 1, 2], [10, null, 30]))).toEqual({ latest: 30, min: 10, max: 30, avg: 20 });
    expect(seriesStats(s("a", [0], [null]))).toEqual({ latest: null, min: null, max: null, avg: null });
  });
});
