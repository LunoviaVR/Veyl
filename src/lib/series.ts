// Combines stored history series that share a chart into uPlot's aligned format.
//
// Rust marks a period with no recorded data by inserting one point whose values
// are null. Those stay `null` (uPlot breaks the line there). A timestamp that
// simply belongs to a *different* series in the same chart becomes `undefined`,
// which uPlot connects across — otherwise toggling one metric on would break the
// lines of every other metric in the chart.
import type { AlignedData } from "uplot";
import type { Series } from "./types";

export interface Merged {
  data: AlignedData;
  /** Periods with no recorded data, in seconds: [last point before, first point after]. */
  gaps: [number, number][];
}

export function mergeSeries(list: Series[]): Merged {
  const ts = [...new Set(list.flatMap((s) => s.t))].sort((a, b) => a - b);
  const index = new Map(ts.map((t, i) => [t, i]));
  const cols = list.map((s) => {
    const col: (number | null | undefined)[] = new Array(ts.length).fill(undefined);
    s.t.forEach((t, i) => {
      col[index.get(t)!] = s.avg[i] ?? null;
    });
    return col;
  });

  // A gap is a timestamp where every series that has a point there has null.
  const gaps: [number, number][] = [];
  for (let i = 0; i < ts.length; i++) {
    const vals = cols.map((c) => c[i]).filter((v) => v !== undefined);
    if (vals.length > 0 && vals.every((v) => v === null)) {
      const before = ts[i - 1];
      const after = ts[i + 1];
      if (before !== undefined && after !== undefined) gaps.push([before / 1000, after / 1000]);
    }
  }
  return { data: [ts.map((t) => t / 1000), ...cols] as AlignedData, gaps };
}

export interface SeriesStats {
  latest: number | null;
  min: number | null;
  max: number | null;
  avg: number | null;
}

/** Latest/min/avg/max over recorded points only (gaps excluded). */
export function seriesStats(s: Series): SeriesStats {
  const vals: number[] = [];
  let latest: number | null = null;
  s.avg.forEach((v, i) => {
    if (v == null) return;
    vals.push(v);
    latest = v;
    void i;
  });
  if (!vals.length) return { latest: null, min: null, max: null, avg: null };
  const mins = s.min.filter((v): v is number => v != null);
  const maxs = s.max.filter((v): v is number => v != null);
  return {
    latest,
    min: mins.length ? Math.min(...mins) : Math.min(...vals),
    max: maxs.length ? Math.max(...maxs) : Math.max(...vals),
    avg: vals.reduce((a, b) => a + b, 0) / vals.length,
  };
}
