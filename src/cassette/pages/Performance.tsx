import { useEffect, useMemo } from "react";
import type { AlignedData } from "uplot";
import { useNav } from "../App";
import { Chart, type ChartSeries, Sparkline, useThemeKey } from "../components/Chart";
import { Empty, Fields, Kpi, Lamp, PageHeader, Panel, StatusNote } from "../components/ui";
import * as f from "../../lib/format";
import { usePref, useQuery } from "../../lib/hooks";
import { api } from "../../lib/ipc";
import { useLive } from "../../lib/live";
import type { TelemetryFrame } from "../../lib/types";

type Pick = (fr: TelemetryFrame) => number | null | undefined;

function useSeries(ring: TelemetryFrame[], picks: Pick[]): AlignedData {
  return useMemo(() => [ring.map((r) => r.timestampMs / 1000), ...picks.map((p) => ring.map((r) => p(r) ?? null))] as AlignedData, [ring, picks]);
}

function LiveChart({ ring, picks, series, yMax, height = 220 }: { ring: TelemetryFrame[]; picks: Pick[]; series: ChartSeries[]; yMax?: number; height?: number }) {
  const data = useSeries(ring, picks);
  return <Chart data={data} series={series} yMax={yMax} height={height} themeKey={useThemeKey()} />;
}

const CPU_PICKS: Pick[] = [(r) => r.cpu.totalPct, (r) => r.cpu.utilityPct];
const MEM_PICKS: Pick[] = [(r) => r.memory.loadPct];
const NET_PICKS: Pick[] = [(r) => r.network.totalRxBps, (r) => r.network.totalTxBps];

function CpuView({ ring, frame }: { ring: TelemetryFrame[]; frame: TelemetryFrame }) {
  const c = frame.cpu;
  // Why the package temperature is missing comes from the CPU temperature provider itself.
  const sensors = useQuery(() => api.sensors(), [c.packageTempC == null]);
  const tempWhy = sensors.data?.providers.find((p) => p.id === "pawnio-cpu")?.status.message;
  return (
    <>
      <Panel title="Utilization (last 5 minutes)">
        <LiveChart ring={ring} picks={CPU_PICKS} yMax={Math.max(100, ...ring.map((r) => r.cpu.utilityPct ?? 0))} series={[{ label: "% Processor Time", color: 1, unit: "%", fill: true }, { label: "% Processor Utility", color: 2, unit: "%", dash: true }]} />
        <p className="text-xs text-faint mt-2">Processor Time is the time-based busy share. Processor Utility is scaled by actual clock speed relative to base and is what Task Manager shows; it can exceed 100% under boost.</p>
      </Panel>
      <Panel title="Logical processors">
        <div className="grid gap-2" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(120px, 1fr))" }}>
          {c.perCorePct.map((v, i) => (
            <div key={i} className="border border-line rounded-sm p-1.5" title={v == null ? "Windows returned no valid value for this processor in the last sample (common for deeply idle cores)" : undefined}>
              <div className="flex justify-between text-2xs text-muted tabular">
                <span>CPU {i}</span>
                <span>{f.pct(v)}</span>
              </div>
              <Sparkline values={ring.slice(-60).map((r) => r.cpu.perCorePct[i] ?? null)} max={100} height={26} />
              <div className="text-2xs text-faint tabular">{f.mhz(c.perCoreMhz[i])}</div>
            </div>
          ))}
        </div>
      </Panel>
      <Panel title="Details">
        <Fields
          rows={[
            ["Utilization", f.pct(c.totalPct, 1)],
            ["Utility", f.pct(c.utilityPct, 1)],
            ["Base speed", f.mhz(c.baseMhz)],
            ["Effective speed", f.mhz(c.effectiveMhz)],
            ["Package temperature", c.packageTempC != null ? `${f.temp(c.packageTempC)} (${c.packageTempSource})` : `Not available${tempWhy ? ` — ${tempWhy}` : ""}`],
            ["Package power", c.packagePowerW != null ? `${c.packagePowerW.toFixed(1)} W (RAPL, Windows Energy Meter)` : "Not published by this system"],
            ["Processes / threads / handles", `${f.num(c.processCount)} / ${f.num(c.threadCount)} / ${f.num(c.handleCount)}`],
            ["Up time", f.duration(c.uptimeSecs)],
          ]}
        />
        <div className="mt-2">
          <StatusNote status={c.status} />
        </div>
      </Panel>
    </>
  );
}

function MemoryView({ ring, frame }: { ring: TelemetryFrame[]; frame: TelemetryFrame }) {
  const m = frame.memory;
  const commitPct = m.commitBytes != null && m.commitLimitBytes ? (m.commitBytes / m.commitLimitBytes) * 100 : null;
  const commitPicks = useMemo<Pick[]>(() => [(r) => (r.memory.commitBytes != null && r.memory.commitLimitBytes ? (r.memory.commitBytes / r.memory.commitLimitBytes) * 100 : null)], []);
  return (
    <>
      <Panel title="Physical memory in use">
        <LiveChart ring={ring} picks={MEM_PICKS} yMax={100} series={[{ label: "Memory load", color: 1, unit: "%", fill: true }]} />
      </Panel>
      <Panel title="Committed memory (of commit limit)">
        <LiveChart ring={ring} picks={commitPicks} yMax={100} height={140} series={[{ label: "Commit", color: 1, unit: "%" }]} />
      </Panel>
      <Panel title="Details">
        <Fields
          rows={[
            ["Installed (usable)", f.bytes(m.totalBytes)],
            ["In use", f.bytes(m.usedBytes)],
            ["Available", f.bytes(m.availableBytes)],
            ["Committed", `${f.bytes(m.commitBytes)} of ${f.bytes(m.commitLimitBytes)} (${f.pct(commitPct)})`],
            ["Cached", f.bytes(m.cachedBytes)],
            ["Paged / non-paged pool", `${f.bytes(m.pagedPoolBytes)} / ${f.bytes(m.nonpagedPoolBytes)}`],
            ["Hard faults", `${f.num(m.pagesInputPerSec)} pages/s`],
          ]}
        />
      </Panel>
    </>
  );
}

function DiskView({ ring, frame, idx }: { ring: TelemetryFrame[]; frame: TelemetryFrame; idx: number }) {
  const d = frame.disks.physical[idx];
  const picks = useMemo<Pick[]>(() => [(r) => r.disks.physical[idx]?.readBps, (r) => r.disks.physical[idx]?.writeBps], [idx]);
  const active = useMemo<Pick[]>(() => [(r) => r.disks.physical[idx]?.activePct], [idx]);
  if (!d) return <StatusNote status={frame.disks.status} always />;
  return (
    <>
      <Panel title="Transfer rate">
        <LiveChart ring={ring} picks={picks} series={[{ label: "Read", color: 1, unit: "B/s", fill: true }, { label: "Write", color: 2, unit: "B/s" }]} />
      </Panel>
      <Panel title="Active time">
        <LiveChart ring={ring} picks={active} yMax={100} height={120} series={[{ label: "Active", color: 1, unit: "%", fill: true }]} />
      </Panel>
      <Panel title="Details">
        <Fields rows={[["Disk", d.name], ["Read", f.rate(d.readBps)], ["Write", f.rate(d.writeBps)], ["Active time", f.pct(d.activePct)], ["Queue length", f.num(d.queueLength, 1)]]} />
      </Panel>
    </>
  );
}

function NetworkView({ ring, frame }: { ring: TelemetryFrame[]; frame: TelemetryFrame }) {
  return (
    <>
      <Panel title="Throughput (all physical adapters)">
        <LiveChart ring={ring} picks={NET_PICKS} series={[{ label: "Receive", color: 1, unit: "B/s", fill: true }, { label: "Send", color: 2, unit: "B/s" }]} />
      </Panel>
      <Panel title="Adapters">
        <div className="flex flex-col gap-2">
          {frame.network.adapters.map((a) => (
            <div key={a.luid} className="grid grid-cols-[1fr_auto] gap-x-4 text-sm border-b border-line/60 pb-2">
              <div className="min-w-0">
                <div className="font-medium truncate">{a.alias}</div>
                <div className="text-muted truncate">{a.description}</div>
              </div>
              <div className="text-right tabular">
                <div>
                  ↓ {f.bitRate(a.rxBps)} · ↑ {f.bitRate(a.txBps)}
                </div>
                <div className={a.connected ? "text-ok" : "text-faint"}>
                  {a.connected ? `${a.media} · ${f.bitRate(a.linkSpeedBps)}` : "Disconnected"}
                </div>
              </div>
            </div>
          ))}
        </div>
        <StatusNote status={frame.network.status} />
      </Panel>
    </>
  );
}

function GpuView({ ring, frame, idx }: { ring: TelemetryFrame[]; frame: TelemetryFrame; idx: number }) {
  const g = frame.gpus.adapters[idx];
  const util = useMemo<Pick[]>(() => [(r) => r.gpus.adapters[idx]?.utilPct], [idx]);
  const temps = useMemo<Pick[]>(() => [(r) => r.gpus.adapters[idx]?.temperatureC, (r) => r.gpus.adapters[idx]?.hotspotC], [idx]);
  const vram = useMemo<Pick[]>(() => [(r) => r.gpus.adapters[idx]?.dedicatedUsedBytes], [idx]);
  if (!g) return <StatusNote status={frame.gpus.status} always />;
  return (
    <>
      <Panel title="Utilization (busiest engine)">
        <LiveChart ring={ring} picks={util} yMax={100} series={[{ label: "GPU", color: 1, unit: "%", fill: true }]} />
        <div className="flex flex-wrap gap-x-4 gap-y-1 mt-2 text-xs text-muted tabular">
          {g.engines.map((e) => (
            <span key={e.engineType}>
              {e.engineType} {f.pct(e.utilPct)}
            </span>
          ))}
        </div>
      </Panel>
      <div className="grid grid-cols-1 xl:grid-cols-2 gap-3">
        <Panel title="Temperature">
          <LiveChart ring={ring} picks={temps} height={140} series={[{ label: "GPU", color: 1, unit: "°C" }, { label: "Hotspot", color: 2, unit: "°C", dash: true }]} />
        </Panel>
        <Panel title="Dedicated memory">
          <LiveChart ring={ring} picks={vram} height={140} yMax={g.dedicatedTotalBytes || undefined} series={[{ label: "VRAM used", color: 1, unit: "B", fill: true }]} />
        </Panel>
      </div>
      <Panel title="Details">
        <Fields
          rows={[
            ["Adapter", g.name],
            ["Utilization", f.pct(g.utilPct, 1)],
            ["Temperature", f.temp(g.temperatureC)],
            ["Hotspot", g.hotspotC != null ? f.temp(g.hotspotC) : "Not reported (needs a vendor GPU provider)"],
            ["Rated maximum", f.temp(g.temperatureMaxC)],
            ["Core clock (3D engine)", f.mhz(g.coreClockMhz)],
            ["Memory clock", f.mhz(g.memoryClockMhz)],
            ["Fan", g.fanRpm != null ? `${g.fanRpm} RPM` : "Not reported"],
            ["Power", g.powerW != null ? `${g.powerW.toFixed(1)} W board power` : g.powerPct != null ? `${f.pct(g.powerPct, 1)} of board limit` : "Not reported by the driver"],
            ["Dedicated memory", `${f.bytes(g.dedicatedUsedBytes)} of ${f.bytes(g.dedicatedTotalBytes)}`],
            ["Shared memory in use", f.bytes(g.sharedUsedBytes)],
          ]}
        />
        <div className="mt-2 flex flex-col gap-1">
          <StatusNote status={g.countersStatus} />
          <StatusNote status={g.sensorsStatus} />
        </div>
      </Panel>
    </>
  );
}

export default function Performance() {
  const { params } = useNav();
  const ring = useLive((s) => s.ring);
  const frame = useLive((s) => s.frame);
  const [item, setItem] = usePref<string>("perfItem", "cpu");
  useEffect(() => {
    if (params.item) setItem(String(params.item));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params]);
  const current = item;

  if (!frame)
    return (
      <div className="flex-1 min-h-0 flex flex-col">
        <PageHeader title="Performance" subtitle="Live view of the last five minutes; long-term data is in History" />
        <div className="crt mx-5">
          <Empty title="Waiting for the first sample">Readings appear a second or two after Veyl starts.</Empty>
        </div>
      </div>
    );
  const entries: { id: string; label: string; sub: string; spark: (number | null)[]; max?: number }[] = [
    { id: "cpu", label: "CPU", sub: `${f.pct(frame.cpu.totalPct)} · ${f.mhz(frame.cpu.effectiveMhz)}`, spark: ring.slice(-40).map((r) => r.cpu.totalPct), max: 100 },
    { id: "memory", label: "Memory", sub: `${f.bytes(frame.memory.usedBytes)} / ${f.bytes(frame.memory.totalBytes)}`, spark: ring.slice(-40).map((r) => r.memory.loadPct), max: 100 },
    ...frame.disks.physical.map((d, i) => ({
      id: `disk-${i}`,
      label: `Disk ${d.name}`,
      sub: `${f.pct(d.activePct)} active`,
      spark: ring.slice(-40).map((r) => r.disks.physical[i]?.activePct ?? null),
      max: 100,
    })),
    { id: "network", label: "Network", sub: `↓ ${f.bitRate(frame.network.totalRxBps)}`, spark: ring.slice(-40).map((r) => r.network.totalRxBps) },
    ...frame.gpus.adapters.map((g, i) => ({
      id: `gpu-${i}`,
      label: `GPU ${i}`,
      sub: `${g.name} · ${f.pct(g.utilPct)}${g.temperatureC != null ? ` · ${f.temp(g.temperatureC)}` : ""}`,
      spark: ring.slice(-40).map((r) => r.gpus.adapters[i]?.utilPct ?? null),
      max: 100,
    })),
  ];
  const sel = entries.some((e) => e.id === current) ? current : "cpu";
  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <PageHeader title="Performance" subtitle="Live view of the last five minutes; long-term data is in History" />
      <div className="flex-1 min-h-0 flex gap-4 px-5 pb-5">
        <div className="w-64 shrink-0 flex flex-col gap-2 overflow-y-auto pr-1 pb-1" role="listbox" aria-label="Devices">
          {entries.map((e) => (
            <button
              key={e.id}
              role="option"
              aria-selected={sel === e.id}
              onClick={() => setItem(e.id)}
              className="crt text-left px-3 py-2 shrink-0 hover:brightness-125 aria-selected:border-accent"
            >
              <div className="flex items-center gap-2">
                <Lamp tone={sel === e.id ? "ok" : null} />
                <span className="label text-xs truncate">{e.label}</span>
              </div>
              <div className="text-xs text-muted truncate tabular">{e.sub}</div>
              <Sparkline values={e.spark} max={e.max} height={24} />
            </button>
          ))}
        </div>
        <div className="flex-1 min-w-0 overflow-y-auto flex flex-col gap-4 pr-1 pb-1">
          {sel === "cpu" && (
            <div className="crt grid grid-cols-4 gap-3 px-4 py-3 shrink-0">
              <Kpi label="Utilization" value={f.pct(frame.cpu.totalPct)} />
              <Kpi label="Speed" value={f.mhz(frame.cpu.effectiveMhz)} />
              <Kpi label="Processes" value={f.num(frame.cpu.processCount)} />
              <Kpi label="Threads" value={f.num(frame.cpu.threadCount)} />
            </div>
          )}
          {sel === "cpu" && <CpuView ring={ring} frame={frame} />}
          {sel === "memory" && <MemoryView ring={ring} frame={frame} />}
          {sel.startsWith("disk-") && <DiskView ring={ring} frame={frame} idx={Number(sel.slice(5))} />}
          {sel === "network" && <NetworkView ring={ring} frame={frame} />}
          {sel.startsWith("gpu-") && <GpuView ring={ring} frame={frame} idx={Number(sel.slice(4))} />}
        </div>
      </div>
    </div>
  );
}
