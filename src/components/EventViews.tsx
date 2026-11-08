// Shared views for Windows events and the telemetry recorded around them.
import { useMemo } from "react";
import * as f from "../lib/format";
import { mergeSeries } from "../lib/series";
import type { EventKind, Series, StabilityEvent, TelemetryContext } from "../lib/types";
import { Chart, useThemeKey } from "./Chart";
import { SeverityBadge } from "./ui";

export const KIND_LABEL: Record<EventKind, string> = {
  appCrash: "Application crash",
  appHang: "Application hang",
  werReport: "Error report",
  bugcheck: "Blue screen",
  kernelPowerUnexpected: "Unexpected restart",
  unexpectedShutdown: "Unexpected shutdown",
  wheaError: "Hardware error",
  wheaCorrected: "Corrected hardware error",
  gpuDriverReset: "GPU driver reset",
  storageError: "Storage warning",
  serviceFailure: "Service failure",
  systemStart: "System start",
  systemShutdown: "System shutdown",
};

const LABELS: Record<string, string> = {
  "cpu.total_pct": "CPU",
  "memory.load_pct": "Memory",
  "memory.commit_pct": "Commit",
  "gpu.0.util_pct": "GPU",
  "gpu.0.temp_c": "GPU temp",
  "cpu.package_temp_c": "CPU temp",
  "gpu.0.vram_used_bytes": "VRAM",
};

function mergeByUnit(series: Series[]) {
  const byUnit = new Map<string, Series[]>();
  for (const s of series) byUnit.set(s.unit, [...(byUnit.get(s.unit) ?? []), s]);
  return [...byUnit.entries()].map(([unit, list]) => ({ unit, ...mergeSeries(list), labels: list.map((s) => LABELS[s.key] ?? s.key) }));
}

/** Charts the telemetry recorded around an incident; says so explicitly when nothing was recorded. */
export function ContextCharts({ ctx, markerMs }: { ctx: TelemetryContext; markerMs?: number }) {
  const theme = useThemeKey();
  const groups = useMemo(() => mergeByUnit(ctx.series), [ctx]);
  if (ctx.empty) {
    return (
      <p className="text-[12.5px] text-muted">
        No telemetry was recorded between {f.dateTime(ctx.fromMs)} and {f.dateTime(ctx.toMs)}. PC Health Console was not running or history recording was off, so system load and temperatures at that time are unknown.
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-3">
      {markerMs != null && <p className="text-[11.5px] text-faint">Event at {f.timeOnly(markerMs)} — window {f.timeOnly(ctx.fromMs)} to {f.timeOnly(ctx.toMs)}. Gaps mean no data was recorded.</p>}
      {groups.map((g) => (
        <Chart
          key={g.unit}
          data={g.data}
          gaps={g.gaps}
          height={120}
          yMax={g.unit === "%" ? 100 : undefined}
          series={g.labels.map((l, i) => ({ label: l, color: (i % 6) + 1, unit: g.unit }))}
          themeKey={theme}
        />
      ))}
    </div>
  );
}

export function EventRow({ e, onOpen }: { e: StabilityEvent; onOpen?: () => void }) {
  return (
    <button onClick={onOpen} disabled={!onOpen} className="w-full grid grid-cols-[172px_86px_1fr_auto] items-center gap-3 px-3 h-8 text-left text-[12.5px] border-b border-line/50 hover:bg-hover disabled:hover:bg-transparent">
      <span className="text-muted tabular whitespace-nowrap truncate">{f.dateTime(e.timeMs)}</span>
      <SeverityBadge severity={e.severity} />
      <span className="truncate">{e.title}</span>
      <span className="text-faint text-[11.5px]">
        {e.provider} · {e.eventId}
      </span>
    </button>
  );
}

export function RawData({ data }: { data: [string, string][] }) {
  if (!data.length) return null;
  return (
    <details className="text-[12px]">
      <summary className="cursor-pointer text-muted">Event data ({data.length} fields)</summary>
      <table className="mt-1 w-full">
        <tbody>
          {data.map(([k, v], i) => (
            <tr key={`${k}${i}`} className="border-t border-line/50 align-top">
              <td className="pr-3 py-0.5 text-muted whitespace-nowrap">{k}</td>
              <td className="py-0.5 font-mono text-[11.5px] break-all selectable whitespace-pre-wrap">{v}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </details>
  );
}
