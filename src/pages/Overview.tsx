import { ArrowRight, Cpu, HardDrive, MemoryStick, MonitorCog, Network, ShieldCheck } from "lucide-react";
import type { ReactNode } from "react";
import { useNav } from "../App";
import { Sparkline } from "../components/Chart";
import { Button, cn, Fields, Kpi, Meter, PageHeader, Panel, SeverityBadge, StatusNote } from "../components/ui";
import * as f from "../lib/format";
import { useQuery } from "../lib/hooks";
import { api } from "../lib/ipc";
import { useLive } from "../lib/live";
import type { StabilityEvent, TelemetryFrame } from "../lib/types";

const DAY = 86_400_000;

function Tile({ icon, title, children, onOpen }: { icon: ReactNode; title: string; children: ReactNode; onOpen?: () => void }) {
  return (
    <Panel
      title={
        <span className="flex items-center gap-1.5 normal-case tracking-normal text-[12.5px] text-fg">
          {icon}
          {title}
        </span>
      }
      actions={
        onOpen && (
          <Button variant="ghost" onClick={onOpen} aria-label={`Open ${title}`}>
            <ArrowRight className="size-3.5" />
          </Button>
        )
      }
      bodyClass="flex flex-col gap-3"
    >
      {children}
    </Panel>
  );
}

function series(ring: TelemetryFrame[], pick: (f: TelemetryFrame) => number | null | undefined, n = 90) {
  return ring.slice(-n).map((fr) => pick(fr) ?? null);
}

export default function Overview() {
  const { go } = useNav();
  const frame = useLive((s) => s.frame);
  const ring = useLive((s) => s.ring);
  const info = useQuery(() => api.systemInfo(), []);
  const now = Date.now();
  const events = useQuery(() => api.stabilityEvents(now - 30 * DAY, now + DAY, 5000), [], { events: ["stability-changed"] });
  const findings = useQuery(() => api.diagnostics(), [], { events: ["stability-changed"], intervalMs: 300_000 });
  const alerts = useQuery(() => api.activeAlerts(), [], { intervalMs: 5000, events: ["alert"] });
  // Incidents are grouped in Rust: one blue screen leaves several records (WER report + Kernel-Power 41).
  const incidents = useQuery(() => api.bsodOverview().then((o) => o.incidents), [], { events: ["stability-changed"] });

  const cpu = frame?.cpu;
  const mem = frame?.memory;
  const gpu = frame?.gpus.adapters[0];
  const sys = info.data;

  const count = (kinds: StabilityEvent["kind"][], days: number) => events.data?.filter((e) => kinds.includes(e.kind) && e.timeMs >= now - days * DAY).length;
  const crashes7 = count(["appCrash", "appHang"], 7);
  const recentIncidents = incidents.data?.filter((i) => i.timeMs >= now - 30 * DAY);
  const bsod30 = recentIncidents?.filter((i) => i.kind === "confirmedBugcheck").length;
  const unexpected30 = recentIncidents?.filter((i) => i.kind === "unexpectedShutdown").length;
  const whea30 = count(["wheaError", "wheaCorrected"], 30);

  return (
    <div className="flex-1 overflow-y-auto">
      <PageHeader
        title="Overview"
        subtitle={sys ? [sys.computerName, sys.osName, sys.osBuild && `build ${sys.osBuild}`].filter(Boolean).join(" · ") : "Loading system information…"}
        actions={sys?.bootTimeMs && <span className="text-muted text-[12px]">Up {f.duration(cpu?.uptimeSecs)}</span>}
      />
      <div className="grid gap-3 px-5 pb-5 grid-cols-1 lg:grid-cols-2 2xl:grid-cols-3">
        <Tile icon={<Cpu className="size-4 text-muted" />} title={sys?.cpu.brand ?? "Processor"} onOpen={() => go("performance", { item: "cpu" })}>
          <div className="grid grid-cols-4 gap-3">
            <Kpi label="Utilization" value={f.pct(cpu?.totalPct)} sub={`Utility ${f.pct(cpu?.utilityPct)}`} />
            <Kpi label="Effective clock" value={f.mhz(cpu?.effectiveMhz)} sub={`Base ${f.mhz(cpu?.baseMhz)}`} />
            {cpu?.packageTempC != null ? <Kpi label="Package temp" value={f.temp(cpu.packageTempC)} sub={cpu.packageTempSource ?? undefined} /> : <Kpi label="Package power" value={cpu?.packagePowerW != null ? `${cpu.packagePowerW.toFixed(1)} W` : f.DASH} sub="RAPL via Windows" />}
            <Kpi label="Cores / threads" value={`${f.num(sys?.cpu.physicalCores)} / ${f.num(sys?.cpu.logicalProcessors)}`} sub={sys?.cpu.performanceCores ? `${sys.cpu.performanceCores}P + ${sys.cpu.efficiencyCores}E` : undefined} />
          </div>
          <Sparkline values={series(ring, (x) => x.cpu.totalPct)} max={100} color={1} />
          <div className="grid gap-[3px]" style={{ gridTemplateColumns: `repeat(${Math.min(cpu?.perCorePct.length || 1, 24)}, minmax(0,1fr))` }} aria-label="Per-core utilization">
            {cpu?.perCorePct.map((v, i) => (
              <div key={i} className="h-5 rounded-[2px] bg-raised overflow-hidden flex items-end" title={`CPU ${i}: ${f.pct(v)}`}>
                {v != null && <div className="w-full bg-chart-1/80" style={{ height: `${Math.max(2, v)}%` }} />}
              </div>
            ))}
          </div>
          <StatusNote status={cpu?.status} />
        </Tile>

        <Tile icon={<MonitorCog className="size-4 text-muted" />} title={gpu?.name ?? "Graphics"} onOpen={() => go("performance", { item: "gpu-0" })}>
          {gpu ? (
            <>
              <div className="grid grid-cols-4 gap-3">
                <Kpi label="Utilization" value={f.pct(gpu.utilPct)} sub={gpu.engines.find((e) => e.engineType === "3D") ? `3D ${f.pct(gpu.engines.find((e) => e.engineType === "3D")?.utilPct)}` : undefined} />
                <Kpi label="Temperature" value={f.temp(gpu.temperatureC)} sub={gpu.hotspotC != null ? `Hotspot ${f.temp(gpu.hotspotC)}` : "Hotspot not reported"} tone={(gpu.temperatureC ?? 0) >= 90 ? "bad" : undefined} />
                <Kpi label="VRAM" value={f.bytes(gpu.dedicatedUsedBytes)} sub={`of ${f.bytes(gpu.dedicatedTotalBytes)}`} />
                <Kpi label="Core / memory clock" value={f.mhz(gpu.coreClockMhz)} sub={f.mhz(gpu.memoryClockMhz)} />
              </div>
              <Sparkline values={series(ring, (x) => x.gpus.adapters[0]?.utilPct)} max={100} color={2} />
              <Meter value={gpu.dedicatedUsedBytes} max={gpu.dedicatedTotalBytes || 1} />
              <div className="flex gap-4 text-[12px] text-muted tabular">
                <span>Fan {gpu.fanRpm != null ? `${gpu.fanRpm} RPM` : f.DASH}</span>
                <span>Power {gpu.powerW != null ? `${gpu.powerW.toFixed(0)} W` : gpu.powerPct != null ? `${f.pct(gpu.powerPct)} of limit` : "not reported"}</span>
                {gpu.temperatureMaxC != null && <span>Max rated {f.temp(gpu.temperatureMaxC)}</span>}
              </div>
              <StatusNote status={gpu.countersStatus} />
              <StatusNote status={gpu.sensorsStatus} />
            </>
          ) : (
            <StatusNote status={frame?.gpus.status} always />
          )}
        </Tile>

        <Tile icon={<MemoryStick className="size-4 text-muted" />} title="Memory" onOpen={() => go("performance", { item: "memory" })}>
          <div className="grid grid-cols-4 gap-3">
            <Kpi label="In use" value={f.bytes(mem?.usedBytes)} sub={`of ${f.bytes(mem?.totalBytes)}`} tone={(mem?.loadPct ?? 0) >= 90 ? "bad" : undefined} />
            <Kpi label="Available" value={f.bytes(mem?.availableBytes)} />
            <Kpi label="Committed" value={f.bytes(mem?.commitBytes)} sub={`limit ${f.bytes(mem?.commitLimitBytes)}`} />
            <Kpi label="Hard faults" value={f.num(mem?.pagesInputPerSec)} sub="pages read from disk/s" />
          </div>
          <Sparkline values={series(ring, (x) => x.memory.loadPct)} max={100} color={4} />
          <Meter value={mem?.loadPct} />
          <StatusNote status={mem?.status} />
        </Tile>

        <Tile icon={<HardDrive className="size-4 text-muted" />} title="Storage" onOpen={() => go("hardware", { tab: "storage" })}>
          <div className="grid grid-cols-2 gap-3">
            <Kpi label="Read" value={f.rate(frame?.disks.totalReadBps)} />
            <Kpi label="Write" value={f.rate(frame?.disks.totalWriteBps)} />
          </div>
          <div className="flex flex-col gap-2">
            {frame?.disks.volumes
              .filter((v) => v.totalBytes)
              .map((v) => {
                const used = (v.totalBytes ?? 0) - (v.freeBytes ?? 0);
                const freePct = v.totalBytes ? ((v.freeBytes ?? 0) / v.totalBytes) * 100 : null;
                return (
                  <div key={v.mount} className="grid grid-cols-[48px_1fr_150px] items-center gap-2 text-[12px]">
                    <span className="font-medium">{v.mount.replace("\\", "")}</span>
                    <Meter value={used} max={v.totalBytes ?? 1} />
                    <span className={cn("text-right tabular", freePct != null && freePct < 10 ? "text-warn" : "text-muted")}>
                      {f.bytes(v.freeBytes)} free of {f.bytes(v.totalBytes)}
                    </span>
                  </div>
                );
              })}
          </div>
          <StatusNote status={frame?.disks.status} />
        </Tile>

        <Tile icon={<Network className="size-4 text-muted" />} title="Network" onOpen={() => go("performance", { item: "network" })}>
          <div className="grid grid-cols-2 gap-3">
            <Kpi label="Download" value={f.bitRate(frame?.network.totalRxBps)} sub={f.rate(frame?.network.totalRxBps)} />
            <Kpi label="Upload" value={f.bitRate(frame?.network.totalTxBps)} sub={f.rate(frame?.network.totalTxBps)} />
          </div>
          <Sparkline values={series(ring, (x) => x.network.totalRxBps)} color={6} />
          <Fields
            rows={(frame?.network.adapters ?? [])
              .filter((a) => a.connected)
              .slice(0, 3)
              .map((a) => [a.alias, `${a.media} · ${f.bitRate(a.linkSpeedBps)} link`] as [string, string])}
          />
          <StatusNote status={frame?.network.status} />
        </Tile>

        <Tile icon={<ShieldCheck className="size-4 text-muted" />} title="Stability" onOpen={() => go("stability")}>
          <div className="grid grid-cols-4 gap-3">
            <Kpi label="App crashes · 7 d" value={f.num(crashes7)} tone={(crashes7 ?? 0) > 0 ? "warn" : undefined} />
            <Kpi label="Blue screens · 30 d" value={f.num(bsod30)} tone={(bsod30 ?? 0) > 0 ? "bad" : undefined} />
            <Kpi label="Other restarts · 30 d" sub="no stop code" value={f.num(unexpected30)} tone={(unexpected30 ?? 0) > 0 ? "warn" : undefined} />
            <Kpi label="Hardware errors · 30 d" value={f.num(whea30)} tone={(whea30 ?? 0) > 0 ? "bad" : undefined} />
          </div>
          <div className="flex flex-col gap-1.5">
            {(alerts.data ?? []).map((a) => (
              <div key={`${a.ruleId}${a.metric}`} className="flex items-center gap-2 text-[12px]">
                <SeverityBadge severity={a.severity} />
                <span className="truncate">{a.ruleName}</span>
                <span className="ml-auto text-muted tabular">{f.num(a.value, 1)}</span>
              </div>
            ))}
            {(findings.data ?? []).slice(0, 4).map((x) => (
              <button key={x.id} onClick={() => go("stability", { finding: x.id })} className="flex items-center gap-2 text-[12px] text-left hover:text-accent">
                <SeverityBadge severity={x.severity} />
                <span className="truncate">{x.title}</span>
              </button>
            ))}
            {findings.data?.length === 0 && (alerts.data ?? []).length === 0 && <p className="text-[12px] text-muted">No diagnostic findings from recorded evidence.</p>}
            {events.error && <p className="text-[12px] text-bad">{events.error}</p>}
          </div>
        </Tile>
      </div>
      <p className="px-5 pb-4 text-[11.5px] text-faint">
        Monitor overhead: CPU {f.pct(frame?.selfUsage.cpuPct, 1)}, memory {f.bytes(frame?.selfUsage.workingSetBytes)}, sampling {f.ms(frame?.selfUsage.tickCostMs)} per tick.
      </p>
    </div>
  );
}
