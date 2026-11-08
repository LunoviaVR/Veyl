// uPlot wrapper: canvas rendering, efficient in-place data updates, sized to
// its container. `null` values render as gaps (no interpolation across missing data).
import { useEffect, useRef } from "react";
import uPlot, { type AlignedData, type Options } from "uplot";
import { byUnit } from "../lib/format";

export interface ChartSeries {
  label: string;
  /** Theme token index 1..6 (chart palette) */
  color: number;
  unit: string;
  fill?: boolean;
  dash?: boolean;
}

function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
}

export function useThemeKey(): string {
  return document.documentElement.dataset.theme ?? "dark";
}

export function Chart({
  data,
  series,
  height = 160,
  yMax,
  yMin,
  time = true,
  xLabel,
  legend = true,
  themeKey,
  gaps,
}: {
  data: AlignedData;
  series: ChartSeries[];
  height?: number;
  yMax?: number;
  yMin?: number;
  time?: boolean;
  xLabel?: (v: number) => string;
  legend?: boolean;
  themeKey: string;
  /** Periods with no recorded data (x units); shaded and labelled. */
  gaps?: [number, number][];
}) {
  const host = useRef<HTMLDivElement>(null);
  const plot = useRef<uPlot | null>(null);
  const dataRef = useRef(data);
  dataRef.current = data;
  const gapsRef = useRef(gaps);
  gapsRef.current = gaps;

  const seriesKey = series.map((s) => `${s.label}|${s.color}|${s.unit}`).join(",");

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const grid = token("--color-line");
    const axis = token("--color-faint");
    const unit = series[0]?.unit ?? "";
    const shade = token("--color-faint");
    const opts: Options = {
      width: Math.max(el.clientWidth, 100),
      height,
      hooks: {
        drawClear: [
          (u) => {
            const g = gapsRef.current;
            if (!g?.length) return;
            const { ctx, bbox } = u;
            ctx.save();
            ctx.fillStyle = `${shade}22`;
            ctx.font = `${Math.round(10 * devicePixelRatio)}px Segoe UI`;
            // uPlot leaves its axis text alignment on the context; set ours explicitly.
            ctx.textAlign = "left";
            ctx.textBaseline = "alphabetic";
            for (const [a, b] of g) {
              const x0 = Math.max(bbox.left, u.valToPos(a, "x", true));
              const x1 = Math.min(bbox.left + bbox.width, u.valToPos(b, "x", true));
              if (x1 <= x0) continue;
              ctx.fillRect(x0, bbox.top, x1 - x0, bbox.height);
              if (x1 - x0 > 90 * devicePixelRatio) {
                ctx.fillStyle = shade;
                ctx.fillText("not recording", x0 + 6 * devicePixelRatio, bbox.top + 14 * devicePixelRatio);
                ctx.fillStyle = `${shade}22`;
              }
            }
            ctx.restore();
          },
        ],
      },
      legend: { show: legend, live: true },
      cursor: { drag: { x: false, y: false }, points: { size: 5 } },
      scales: {
        x: { time },
        y: {
          range: (_u, dmin, dmax) => {
            const lo = yMin ?? Math.min(0, dmin ?? 0);
            const hi = yMax ?? (dmax == null || dmax <= lo ? lo + 1 : dmax * 1.1);
            return [lo, hi];
          },
        },
      },
      axes: [
        {
          stroke: axis,
          grid: { stroke: grid, width: 1 },
          ticks: { stroke: grid, width: 1 },
          values: xLabel ? (_u, vals) => vals.map((v) => xLabel(v)) : undefined,
          font: "11px Segoe UI",
        },
        {
          stroke: axis,
          grid: { stroke: grid, width: 1 },
          ticks: { show: false },
          size: 58,
          values: (_u, vals) => vals.map((v) => byUnit(v, unit)),
          font: "11px Segoe UI",
        },
      ],
      series: [
        { label: time ? "Time" : "x" },
        ...series.map((s) => {
          const c = token(`--color-chart-${s.color}`);
          return {
            label: s.label,
            stroke: c,
            width: 1.5,
            dash: s.dash ? [4, 3] : undefined,
            fill: s.fill ? `${c}22` : undefined,
            spanGaps: false,
            points: { show: false },
            value: (_u: uPlot, v: number | null) => (v == null ? "—" : byUnit(v, s.unit)),
          };
        }),
      ],
    };
    const u = new uPlot(opts, dataRef.current, el);
    plot.current = u;
    const ro = new ResizeObserver(() => u.setSize({ width: Math.max(el.clientWidth, 100), height }));
    ro.observe(el);
    return () => {
      ro.disconnect();
      u.destroy();
      plot.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seriesKey, height, yMax, yMin, time, themeKey, legend]);

  useEffect(() => {
    const u = plot.current;
    if (!u) return;
    const id = requestAnimationFrame(() => u.setData(data));
    return () => cancelAnimationFrame(id);
  }, [data, gaps]);

  return <div ref={host} className="w-full min-w-0" />;
}

/** Minimal canvas sparkline for dense tiles. */
export function Sparkline({ values, max, color = 1, height = 28 }: { values: (number | null)[]; max?: number; color?: number; height?: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = ref.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth;
    c.width = w * dpr;
    c.height = height * dpr;
    const g = c.getContext("2d");
    if (!g) return;
    g.scale(dpr, dpr);
    g.clearRect(0, 0, w, height);
    const finite = values.filter((v): v is number => v != null && Number.isFinite(v));
    if (finite.length < 2) return;
    const hi = max ?? Math.max(...finite, 1);
    const stroke = token(`--color-chart-${color}`);
    g.strokeStyle = stroke;
    g.fillStyle = `${stroke}1f`;
    g.lineWidth = 1.25;
    const step = w / Math.max(values.length - 1, 1);
    let started = false;
    g.beginPath();
    values.forEach((v, i) => {
      if (v == null) {
        started = false;
        return;
      }
      const x = i * step;
      const y = height - 1 - (Math.min(v, hi) / hi) * (height - 2);
      if (!started) {
        g.moveTo(x, y);
        started = true;
      } else g.lineTo(x, y);
    });
    g.stroke();
  }, [values, max, color, height]);
  return <canvas ref={ref} style={{ width: "100%", height }} aria-hidden />;
}
