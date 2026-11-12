import { Activity, CheckCheck, Cpu, Download, Gauge, HardDrive, MemoryStick, MonitorCog, Network, Thermometer, X } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { useNav } from "../App";
import { Chart, useThemeKey } from "../components/Chart";
import { Button, cn, Empty, ErrorNote, PageHeader, Reels, Switch, TextInput } from "../components/ui";
import * as f from "../../lib/format";
import { usePref, useQuery } from "../../lib/hooks";
import { api, errorText } from "../../lib/ipc";
import { mergeSeries, seriesStats } from "../../lib/series";
import { metricLabel } from "../../pages/History";
import type { MetricInfo, Series } from "../../lib/types";

const RANGES = [
  { id: "15m", label: "15 min", ms: 15 * 60_000 },
  { id: "1h", label: "1 hour", ms: 3_600_000 },
  { id: "24h", label: "24 hours", ms: 86_400_000 },
  { id: "7d", label: "7 days", ms: 7 * 86_400_000 },
  { id: "30d", label: "30 days", ms: 30 * 86_400_000 },
  { id: "custom", label: "Custom", ms: 0 },
];

/** `datetime-local` input value (local time, minutes) for a UTC timestamp. */
const toLocalInput = (ms: number) => new Date(ms - new Date(ms).getTimezoneOffset() * 60_000).toISOString().slice(0, 16);

interface Category {
  id: string;
  label: string;
  icon: ReactNode;
  match: (key: string) => boolean;
}

const CATEGORIES: Category[] = [
  { id: "cpu", label: "Processor", icon: <Cpu className="size-4" />, match: (k) => k.startsWith("cpu.") || k.startsWith("thermal.") },
  { id: "gpu", label: "Graphics", icon: <MonitorCog className="size-4" />, match: (k) => k.startsWith("gpu.") },
  { id: "memory", label: "Memory", icon: <MemoryStick className="size-4" />, match: (k) => k.startsWith("memory.") },
  { id: "storage", label: "Storage", icon: <HardDrive className="size-4" />, match: (k) => k.startsWith("disk.") || k.startsWith("volume.") },
  { id: "network", label: "Network", icon: <Network className="size-4" />, match: (k) => k.startsWith("net.") },
  { id: "sensors", label: "Sensors", icon: <Thermometer className="size-4" />, match: (k) => k.startsWith("sensor.") },
  { id: "self", label: "Monitor overhead", icon: <Gauge className="size-4" />, match: (k) => k.startsWith("self.") },
];

const categoryOf = (key: string) => CATEGORIES.find((c) => c.match(key))?.id ?? "self";

const UNIT_TITLE: Record<string, string> = {
  "%": "Utilization and load",
  "°C": "Temperature",
  B: "Memory",
  "B/s": "Throughput",
  MHz: "Clock speed",
  RPM: "Fan speed",
  W: "Power",
  V: "Voltage",
  ms: "Time",
  "": "Count",
};

function CategoryCard({ cat, metrics, selected, onToggle, onToggleAll, gpuNames }: { cat: Category; metrics: MetricInfo[]; selected: Set<string>; onToggle: (k: string) => void; onToggleAll: (on: boolean) => void; gpuNames: string[] }) {
  const on = metrics.filter((m) => selected.has(m.key)).length;
  return (
    <div className="crt p-3.5 flex flex-col gap-2.5 min-w-0">
      <div className="flex items-center gap-2">
        <span className={on ? "text-accent" : "text-muted"}>{cat.icon}</span>
        <span className="label text-xs">{cat.label}</span>
        <span className="text-xs text-faint tabular">
          {on}/{metrics.length}
        </span>
        <span className="ml-auto">
          <Switch label={`Show all ${cat.label} metrics`} checked={on === metrics.length && on > 0} onChange={onToggleAll} />
        </span>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {metrics.map((m) => {
          const active = selected.has(m.key);
          return (
            <button
              key={m.key}
              aria-pressed={active}
              title={m.key}
              onClick={() => onToggle(m.key)}
              className={cn(
                "h-7 px-2.5 rounded-[3px] border text-xs max-w-full truncate",
                active ? "bg-accent border-accent text-accent-fg" : "border-line-strong text-muted hover:text-fg hover:border-faint",
              )}
            >
              {metricLabel(m.key, gpuNames)}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function StatTable({ list, gpuNames }: { list: Series[]; gpuNames: string[] }) {
  return (
    <table className="w-full text-xs tabular mt-2">
      <thead className="text-faint text-2xs">
        <tr>
          <th className="text-left font-normal py-0.5">Metric</th>
          <th className="text-right font-normal">Latest</th>
          <th className="text-right font-normal">Min</th>
          <th className="text-right font-normal">Avg</th>
          <th className="text-right font-normal">Max</th>
        </tr>
      </thead>
      <tbody>
        {list.map((s, i) => {
          const st = seriesStats(s);
          return (
            <tr key={s.key} className="border-t border-line/50">
              <td className="py-1 pr-2">
                <span className="inline-flex items-center gap-1.5 min-w-0">
                  <span className="size-2.5 rounded-sm shrink-0" style={{ background: `var(--color-chart-${(i % 6) + 1})` }} />
                  <span className="truncate">{metricLabel(s.key, gpuNames)}</span>
                </span>
              </td>
              <td className="text-right">{f.byUnit(st.latest, s.unit)}</td>
              <td className="text-right text-muted">{f.byUnit(st.min, s.unit)}</td>
              <td className="text-right text-muted">{f.byUnit(st.avg, s.unit)}</td>
              <td className="text-right text-muted">{f.byUnit(st.max, s.unit)}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

export default function History() {
  const { settings } = useNav();
  const theme = useThemeKey();
  const catalog = useQuery(() => api.metricCatalog(), []);
  const info = useQuery(() => api.systemInfo(), []);
  const [keys, setKeys] = usePref<string[]>("histKeys", ["cpu.total_pct", "gpu.0.util_pct", "memory.load_pct"]);
  const [range, setRange] = usePref("histRange", "1h");
  // Custom range starts as the last hour so the page is never blank (the range choice persists, these do not).
  const [custom, setCustom] = useState(() => ({ from: toLocalInput(Date.now() - 3_600_000), to: toLocalInput(Date.now()) }));
  const [pickerOpen, setPickerOpen] = usePref("histPicker", true);
  const [error, setError] = useState<string | null>(null);
  const gpuNames = info.data?.gpus.map((g) => g.name) ?? [];
  const available = useMemo(() => new Set((catalog.data ?? []).map((m) => m.key)), [catalog.data]);
  const selected = useMemo(() => new Set(keys.filter((k) => available.size === 0 || available.has(k))), [keys, available]);

  const r = RANGES.find((x) => x.id === range) ?? RANGES[1]!;
  const span = (() => {
    if (r.id !== "custom") {
      const to = Date.now();
      return { from: to - r.ms, to };
    }
    const from = Date.parse(custom.from);
    const to = Date.parse(custom.to);
    return Number.isFinite(from) && Number.isFinite(to) && to > from ? { from, to } : null;
  })();
  // Re-query every 10 s for short ranges; the key changes only when the window moves by 10 s.
  const spanKey = span ? `${Math.floor(span.from / 10_000)}-${Math.floor(span.to / 10_000)}` : "none";
  const sel = [...selected].sort();
  const data = useQuery(() => (span && sel.length ? api.history(sel, span.from, span.to, 900) : Promise.resolve([] as Series[])), [sel.join(","), spanKey], {
    intervalMs: r.ms && r.ms <= 3_600_000 ? 10_000 : undefined,
  });

  const byCategory = useMemo(() => {
    const m = new Map<string, MetricInfo[]>();
    for (const c of catalog.data ?? []) m.set(categoryOf(c.key), [...(m.get(categoryOf(c.key)) ?? []), c]);
    for (const list of m.values()) list.sort((a, b) => metricLabel(a.key, gpuNames).localeCompare(metricLabel(b.key, gpuNames)));
    return m;
  }, [catalog.data, gpuNames]);

  // Charts: one section per category, one chart per unit inside it.
  const sections = useMemo(() => {
    const out: { cat: Category; charts: { unit: string; list: Series[]; merged: ReturnType<typeof mergeSeries> }[] }[] = [];
    for (const cat of CATEGORIES) {
      const list = (data.data ?? []).filter((s) => s.t.length && categoryOf(s.key) === cat.id);
      if (!list.length) continue;
      const byUnit = new Map<string, Series[]>();
      for (const s of list) byUnit.set(s.unit, [...(byUnit.get(s.unit) ?? []), s]);
      out.push({ cat, charts: [...byUnit.entries()].map(([unit, l]) => ({ unit, list: l, merged: mergeSeries(l) })) });
    }
    return out;
  }, [data.data]);

  const toggle = (k: string) => setKeys(selected.has(k) ? [...selected].filter((x) => x !== k) : [...selected, k]);
  const setCategory = (cat: string, on: boolean) => {
    const inCat = (byCategory.get(cat) ?? []).map((m) => m.key);
    setKeys(on ? [...new Set([...selected, ...inCat])] : [...selected].filter((k) => !inCat.includes(k)));
  };
  const resolution = data.data?.[0];

  return (
    <div className="flex-1 overflow-y-auto">
      <PageHeader
        title="History"
        subtitle="Recorded telemetry: 5-second averages for recent data, 1-minute averages long-term. Shaded areas are periods when nothing was being recorded."
        actions={
          <>
            <span className="label flex items-center gap-2 text-2xs text-muted" title="Recording is switched on or off in Settings">
              <Reels spinning={!!settings?.historyEnabled} /> {settings?.historyEnabled === false ? "Recording off" : "Recording"}
            </span>
            <Button disabled={!span || !sel.length} onClick={() => span && api.exportHistoryCsv(sel, span.from, span.to).catch((e) => setError(errorText(e)))}>
              <Download className="size-3.5" /> Export CSV
            </Button>
          </>
        }
      />
      <div className="px-5 pb-6 flex flex-col gap-4">
        <div className="flex flex-wrap items-center gap-1.5 sticky top-0 z-10 bg-bg py-2 -my-2">
          {RANGES.map((x) => (
            <button
              key={x.id}
              onClick={() => {
                if (x.id === "custom" && range !== "custom" && span) setCustom({ from: toLocalInput(span.from), to: toLocalInput(span.to) });
                setRange(x.id);
              }}
              aria-pressed={range === x.id}
              className={cn("key label h-8 px-3 rounded-[4px] border border-line-strong text-xs", range === x.id ? "bg-hover text-fg" : "bg-raised text-muted hover:text-fg")}>
              {x.label}
            </button>
          ))}
          {range === "custom" && (
            <>
              <TextInput type="datetime-local" aria-label="From" value={custom.from} onChange={(e) => setCustom({ ...custom, from: e.target.value })} />
              <span className="text-muted">to</span>
              <TextInput type="datetime-local" aria-label="To" value={custom.to} onChange={(e) => setCustom({ ...custom, to: e.target.value })} />
              {!span && <span className="text-bad text-xs">Choose a start time before the end time</span>}
            </>
          )}
          <span className="mx-1 h-5 w-px bg-line" />
          <Button onClick={() => setKeys([...available])} disabled={!available.size}>
            <CheckCheck className="size-3.5" /> Show everything
          </Button>
          <Button variant="ghost" onClick={() => setKeys([])} disabled={!selected.size}>
            <X className="size-3.5" /> Clear
          </Button>
          <Button variant="ghost" onClick={() => setPickerOpen(!pickerOpen)} aria-expanded={pickerOpen}>
            <Activity className="size-3.5" /> {pickerOpen ? "Hide metric picker" : `Choose metrics (${selected.size})`}
          </Button>
          {resolution && (
            <span className="ml-auto text-xs text-faint">
              {resolution.source === "raw" ? "5-second data" : "1-minute data"} · one point per {f.duration(resolution.bucketMs / 1000)}
            </span>
          )}
        </div>

        {pickerOpen && (
          <div className="grid gap-3 grid-cols-1 md:grid-cols-2 2xl:grid-cols-3">
            {CATEGORIES.filter((c) => byCategory.get(c.id)?.length).map((c) => (
              <CategoryCard key={c.id} cat={c} metrics={byCategory.get(c.id)!} selected={selected} onToggle={toggle} onToggleAll={(on) => setCategory(c.id, on)} gpuNames={gpuNames} />
            ))}
          </div>
        )}

        <ErrorNote error={error ?? data.error ?? catalog.error} />
        {catalog.data?.length === 0 && (
          <div className="crt">
            <Empty title="No history recorded yet">History starts recording a few seconds after the application starts.</Empty>
          </div>
        )}
        {!!catalog.data?.length && !selected.size && (
          <div className="crt">
            <Empty title="Choose metrics">Select metrics above, or use “Show everything”.</Empty>
          </div>
        )}
        {selected.size > 0 && sections.length === 0 && !data.loading && (
          <div className="crt">
            <Empty title="No data in this range">Nothing was recorded for the selected metrics in this time range.</Empty>
          </div>
        )}

        {sections.map(({ cat, charts }) => (
          <section key={cat.id} className="crt">
            <header className="flex items-center gap-2 px-4 h-11 border-b border-line">
              <span className="text-muted">{cat.icon}</span>
              <h2 className="label text-sm">{cat.label}</h2>
            </header>
            <div className={cn("grid gap-4 p-4", charts.length > 1 && "xl:grid-cols-2")}>
              {charts.map((c, n) => (
                <div key={c.unit} className="min-w-0 rounded-[4px] border border-line-strong">
                  <h3 className="label flex items-center gap-2 px-3 h-8 bg-raised border-b border-line-strong text-2xs text-fg">
                    <span className="text-faint tabular">CH{String(n + 1).padStart(2, "0")}</span>
                    {UNIT_TITLE[c.unit] ?? c.unit}
                  </h3>
                  <div className="p-3">
                    <Chart
                      data={c.merged.data}
                      gaps={c.merged.gaps}
                      height={180}
                      legend={false}
                      yMax={c.unit === "%" ? 100 : undefined}
                      series={c.list.map((s, i) => ({ label: metricLabel(s.key, gpuNames), color: (i % 6) + 1, unit: c.unit }))}
                      themeKey={theme}
                    />
                    <StatTable list={c.list} gpuNames={gpuNames} />
                  </div>
                </div>
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
