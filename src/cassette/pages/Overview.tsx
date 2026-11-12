import { ArrowRight, Bell, Cpu, HardDrive, History as HistoryIcon, MemoryStick, MonitorCog, Network } from "lucide-react";
import type { ReactNode } from "react";
import { useNav } from "../App";
import { Sparkline } from "../components/Chart";
import { Button, cn, Dial, Empty, ErrorNote, Fields, Kpi, Lamp, Meter, PageHeader, Reels, SeverityBadge, StatusNote } from "../components/ui";
import { annunciators } from "../../lib/annunciators";
import * as f from "../../lib/format";
import { useQuery } from "../../lib/hooks";
import { api } from "../../lib/ipc";
import { useLive } from "../../lib/live";
import type { StabilityEvent, TelemetryFrame, Tone } from "../../lib/types";

const DAY = 86_400_000;
/** The LED bars' own limits, so a load dial and the bar beside it change colour together. */
const LOAD_ZONES = [
  { from: 75, tone: "warn" as const },
  { from: 90, tone: "bad" as const },
];
const TONE_WORD: Record<Tone, string> = { ok: "normal", info: "notice", warn: "caution", bad: "warning" };
const MESSAGE = "flex items-center gap-3 w-full px-4 h-10 border-b border-line/60 text-sm text-left hover:bg-hover";

/** A bolted housing: label strip on top, then its instruments (dials on the plate, readouts on a screen). */
function Module({ icon, label, name, onOpen, children }: { icon: ReactNode; label: string; name?: string | null; onOpen?: () => void; children: ReactNode }) {
  return (
    <section className="plate flex flex-col gap-3 p-3.5 min-w-0">
      <header className="flex items-center gap-2 px-1 h-6 min-w-0">
        <span className="text-muted shrink-0">{icon}</span>
        <h2 className="label text-sm shrink-0">{label}</h2>
        {name && <span className="text-xs text-faint truncate">{name}</span>}
        {onOpen && (
          <Button variant="ghost" className="ml-auto px-2" onClick={onOpen} aria-label={`Open ${label}`}>
            <ArrowRight className="size-4" />
          </Button>
        )}
      </header>
      <div className="flex gap-3 min-w-0 flex-1">{children}</div>
    </section>
  );
}

const SCREEN = "crt flex-1 min-w-0 p-4 flex flex-col gap-3";

function trace(ring: TelemetryFrame[], pick: (f: TelemetryFrame) => number | null | undefined, n = 120) {
  return ring.slice(-n).map((fr) => pick(fr) ?? null);
}

export default function Overview() {
  const { go, settings } = useNav();
  const frame = useLive((s) => s.frame);
  const ring = useLive((s) => s.ring);
  const info = useQuery(() => api.systemInfo(), []);
  const now = Date.now();
  const events = useQuery(() => api.stabilityEvents(now - 30 * DAY, now + DAY, 5000), [], { events: ["stability-changed"] });
  const findings = useQuery(() => api.diagnostics(), [], { events: ["stability-changed"], intervalMs: 300_000 });
  const alerts = useQuery(() => api.activeAlerts(), [], { intervalMs: 5000, events: ["alert"] });
  // Incidents are grouped in Rust: one blue screen leaves several records (WER report + Kernel-Power 41).
  const incidents = useQuery(() => api.bsodOverview().then((o) => o.incidents), [], { events: ["stability-changed"] });
  const runtime = useQuery(() => api.runtimeStatus(), [], { intervalMs: 30_000 });

  const cpu = frame?.cpu;
  const mem = frame?.memory;
  const gpu = frame?.gpus.adapters[0];
  const sys = info.data;
  const db = runtime.data?.dbStats;
  const recording = !!frame && !!settings?.historyEnabled;

  const count = (kinds: StabilityEvent["kind"][], days: number) => events.data?.filter((e) => kinds.includes(e.kind) && e.timeMs >= now - days * DAY).length;
  const recentIncidents = incidents.data?.filter((i) => i.timeMs >= now - 30 * DAY);
  const tiles = annunciators(frame, alerts.data ?? [], {
    crashes7: count(["appCrash", "appHang"], 7),
    bsod30: recentIncidents?.filter((i) => i.kind === "confirmedBugcheck").length,
    unexpected30: recentIncidents?.filter((i) => i.kind === "unexpectedShutdown").length,
    whea30: count(["wheaError", "wheaCorrected"], 30),
  });
  const lit = tiles.filter((t) => t.tone);
  const master = (["bad", "warn", "info"] as const).find((t) => lit.some((x) => x.tone === t));
  /** Red band from the owner's own alert rule, so a dial and its alert agree. */
  const ruleZone = (id: string) => {
    const rule = settings?.alertRules.find((r) => r.id === id && r.enabled && r.comparator === "above");
    return rule ? [{ from: rule.threshold, tone: "bad" as const }] : [];
  };
  const shownFindings = (findings.data ?? []).slice(0, 4);
  const quiet = findings.data?.length === 0 && !alerts.data?.length;

  return (
    <div className="@container flex-1 overflow-y-auto">
      <PageHeader
        title="Overview"
        subtitle={sys ? [sys.computerName, sys.osName, sys.osBuild && `build ${sys.osBuild}`].filter(Boolean).join(" · ") : "Loading system information…"}
        actions={
          sys?.bootTimeMs && (
            <span className="screen field readout-sm px-2.5 py-1 rounded-[4px]" title="Time since Windows started">
              Up {f.duration(cpu?.uptimeSecs)}
            </span>
          )
        }
      />
      <div className="flex flex-col gap-4 px-5 pb-6">
        <div className="grid gap-4 grid-cols-1 @5xl:grid-cols-2">
          <section className="plate flex flex-col gap-3 p-3.5 min-w-0" aria-label="Status lamps">
            <header className="flex items-center gap-2.5 px-1 h-6">
              <Lamp tone={frame ? (master ?? "ok") : null} blink={lit.some((t) => t.blink)} />
              <h2 className="label text-sm">{!frame ? "Standing by" : lit.length ? `${lit.length} ${lit.length === 1 ? "needs" : "need"} attention` : "All systems normal"}</h2>
              <span className="ml-auto text-xs text-faint">A dark tile is normal</span>
            </header>
            <div className="grid grid-cols-5 gap-1.5 flex-1">
              {tiles.map((t) => (
                <button
                  key={t.id}
                  className="annun"
                  data-tone={t.tone ?? undefined}
                  data-blink={t.blink ? "" : undefined}
                  onClick={() => go(t.page, t.params)}
                  aria-label={`${t.label}: ${t.tone ? [TONE_WORD[t.tone], t.note].filter(Boolean).join(", ") : "normal"}`}
                >
                  <span className="label text-xs font-bold leading-tight">{t.label}</span>
                  {t.tone && t.note && <span className="text-2xs font-semibold leading-tight">{t.note}</span>}
                </button>
              ))}
            </div>
          </section>

          <Module icon={<Bell className="size-4" />} label="Messages" onOpen={() => go("stability")}>
            <div className="crt flex-1 min-w-0 flex flex-col overflow-hidden">
              {(alerts.data ?? []).map((a) => (
                <button key={`${a.ruleId}${a.metric}`} className={MESSAGE} onClick={() => go("alerts")}>
                  <SeverityBadge severity={a.severity} />
                  <span className="truncate">{a.ruleName}</span>
                  <span className="ml-auto pl-2 text-muted tabular whitespace-nowrap">
                    {f.num(a.value, 1)} · limit {f.num(a.threshold, 1)}
                  </span>
                </button>
              ))}
              {shownFindings.map((x) => (
                <button key={x.id} className={MESSAGE} onClick={() => go("stability", { finding: x.id })}>
                  <SeverityBadge severity={x.severity} />
                  <span className="truncate">{x.title}</span>
                </button>
              ))}
              {(findings.data?.length ?? 0) > shownFindings.length && (
                <button className={cn(MESSAGE, "text-muted")} onClick={() => go("stability")}>
                  + {findings.data!.length - shownFindings.length} more in Stability
                </button>
              )}
              {quiet && <Empty title="No messages">No alert is active and the diagnostic rules found nothing in the recorded evidence.</Empty>}
              {!findings.data && !findings.error && !alerts.data?.length && (
                <p className="px-4 py-3 text-sm text-muted">&gt; Reading the event log…</p>
              )}
              {(findings.error ?? events.error) && <p className="px-4 py-2 text-xs text-bad">{findings.error ?? events.error}</p>}
            </div>
          </Module>
        </div>

        <div className="grid gap-4 grid-cols-1 @5xl:grid-cols-2">
          <Module icon={<Cpu className="size-4" />} label="Processor" name={sys?.cpu.brand} onOpen={() => go("performance", { item: "cpu" })}>
            <div className="flex gap-2 shrink-0">
              <Dial label="Load" value={cpu?.totalPct} display={f.pct(cpu?.totalPct)} zones={LOAD_ZONES} />
              <Dial
                label="Package temp"
                value={cpu?.packageTempC}
                display={f.temp(cpu?.packageTempC)}
                min={20}
                max={110}
                zones={ruleZone("cpu-temp")}
                title={cpu?.packageTempC != null ? (cpu.packageTempSource ?? undefined) : "No CPU temperature is being reported. Hardware › Sensors says why."}
              />
            </div>
            <div className={SCREEN}>
              <div className="grid grid-cols-2 gap-3">
                <Kpi label="Clock" value={f.mhz(cpu?.effectiveMhz)} sub={`Base ${f.mhz(cpu?.baseMhz)}`} />
                <Kpi label="Power" value={cpu?.packagePowerW != null ? `${cpu.packagePowerW.toFixed(0)} W` : f.DASH} sub={`Utility ${f.pct(cpu?.utilityPct)}`} />
              </div>
              <Sparkline values={trace(ring, (x) => x.cpu.totalPct)} max={100} />
              <div className="grid gap-[3px]" style={{ gridTemplateColumns: `repeat(${Math.min(cpu?.perCorePct.length || 1, 24)}, minmax(0,1fr))` }} aria-label="Per-core utilization">
                {cpu?.perCorePct.map((v, i) => (
                  <div key={i} className="h-6 bg-raised flex items-end" title={`CPU ${i}: ${f.pct(v)}`}>
                    {v != null && <div className={cn("w-full", v >= 90 ? "bg-bad" : "bg-chart-1")} style={{ height: `${Math.max(4, v)}%` }} />}
                  </div>
                ))}
              </div>
              <p className="text-2xs text-faint">
                {f.num(sys?.cpu.physicalCores)} cores / {f.num(sys?.cpu.logicalProcessors)} threads
                {sys?.cpu.performanceCores ? ` (${sys.cpu.performanceCores}P + ${sys.cpu.efficiencyCores}E)` : ""}
              </p>
              <StatusNote status={cpu?.status} />
            </div>
          </Module>

          <Module icon={<MonitorCog className="size-4" />} label="Graphics" name={gpu?.name} onOpen={() => go("performance", { item: "gpu-0" })}>
            <div className="flex gap-2 shrink-0">
              <Dial label="Load" value={gpu?.utilPct} display={f.pct(gpu?.utilPct)} zones={LOAD_ZONES} />
              <Dial label="Temperature" value={gpu?.temperatureC} display={f.temp(gpu?.temperatureC)} min={20} max={gpu?.temperatureMaxC ?? 110} zones={ruleZone("gpu-temp")} />
            </div>
            <div className={SCREEN}>
              {gpu ? (
                <>
                  <div className="grid grid-cols-2 gap-3">
                    <Kpi label="VRAM" value={f.bytes(gpu.dedicatedUsedBytes)} sub={`of ${f.bytes(gpu.dedicatedTotalBytes)}`} />
                    <Kpi label="Hotspot" value={f.temp(gpu.hotspotC)} sub={gpu.hotspotC != null ? undefined : "not reported"} />
                    <Kpi label="Power" value={gpu.powerW != null ? `${gpu.powerW.toFixed(0)} W` : gpu.powerPct != null ? f.pct(gpu.powerPct) : f.DASH} sub={gpu.powerW == null && gpu.powerPct != null ? "of limit" : undefined} />
                    <Kpi label="Fan" value={gpu.fanRpm != null ? `${gpu.fanRpm} RPM` : f.DASH} />
                  </div>
                  <Meter value={gpu.dedicatedUsedBytes} max={gpu.dedicatedTotalBytes || 1} />
                  <Sparkline values={trace(ring, (x) => x.gpus.adapters[0]?.utilPct)} max={100} />
                  <p className="text-2xs text-faint tabular">
                    Core {f.mhz(gpu.coreClockMhz)} · memory {f.mhz(gpu.memoryClockMhz)}
                    {gpu.temperatureMaxC != null && ` · rated to ${f.temp(gpu.temperatureMaxC)}`}
                  </p>
                  <StatusNote status={gpu.countersStatus} />
                  <StatusNote status={gpu.sensorsStatus} />
                </>
              ) : (
                <StatusNote status={frame?.gpus.status} always />
              )}
            </div>
          </Module>

          <Module icon={<MemoryStick className="size-4" />} label="Memory" onOpen={() => go("performance", { item: "memory" })}>
            <div className="shrink-0">
              <Dial label="In use" value={mem?.loadPct} display={f.pct(mem?.loadPct)} zones={LOAD_ZONES} />
            </div>
            <div className={SCREEN}>
              <div className="grid grid-cols-2 @7xl:grid-cols-4 gap-3">
                <Kpi label="In use" value={f.bytes(mem?.usedBytes)} sub={`of ${f.bytes(mem?.totalBytes)}`} tone={(mem?.loadPct ?? 0) >= 90 ? "bad" : undefined} />
                <Kpi label="Available" value={f.bytes(mem?.availableBytes)} />
                <Kpi label="Committed" value={f.bytes(mem?.commitBytes)} sub={`limit ${f.bytes(mem?.commitLimitBytes)}`} />
                <Kpi label="Hard faults" value={f.num(mem?.pagesInputPerSec)} sub="pages read from disk/s" />
              </div>
              <Sparkline values={trace(ring, (x) => x.memory.loadPct)} max={100} />
              <StatusNote status={mem?.status} />
            </div>
          </Module>

          <Module icon={<HardDrive className="size-4" />} label="Storage" onOpen={() => go("hardware", { tab: "storage" })}>
            <div className={SCREEN}>
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
                      <div key={v.mount} className="grid grid-cols-[2rem_1fr_auto] items-center gap-3 text-xs">
                        <span className="readout-sm">{v.mount.replace("\\", "")}</span>
                        <Meter value={used} max={v.totalBytes ?? 1} />
                        <span className={cn("text-right tabular whitespace-nowrap", freePct != null && freePct < 10 ? "text-warn" : "text-muted")}>
                          {f.bytes(v.freeBytes)} free of {f.bytes(v.totalBytes)}
                        </span>
                      </div>
                    );
                  })}
              </div>
              <StatusNote status={frame?.disks.status} />
            </div>
          </Module>

          <Module icon={<Network className="size-4" />} label="Network" onOpen={() => go("performance", { item: "network" })}>
            <div className={SCREEN}>
              <div className="grid grid-cols-2 gap-3">
                <Kpi label="Download" value={f.bitRate(frame?.network.totalRxBps)} sub={f.rate(frame?.network.totalRxBps)} />
                <Kpi label="Upload" value={f.bitRate(frame?.network.totalTxBps)} sub={f.rate(frame?.network.totalTxBps)} />
              </div>
              <Sparkline values={trace(ring, (x) => x.network.totalRxBps)} />
              <Fields
                rows={(frame?.network.adapters ?? [])
                  .filter((a) => a.connected)
                  .slice(0, 3)
                  .map((a) => [a.alias, `${a.media} · ${f.bitRate(a.linkSpeedBps)} link`] as [string, string])}
              />
              <StatusNote status={frame?.network.status} />
            </div>
          </Module>

          <Module icon={<HistoryIcon className="size-4" />} label="Data recorder" onOpen={() => go("history")}>
            <div className="flex flex-col items-center justify-center gap-2.5 w-[112px] shrink-0">
              <Reels spinning={recording} className="w-[92px] h-[38px]" />
              <span className="label flex items-center gap-1.5 text-2xs text-muted">
                <Lamp tone={recording ? "bad" : null} /> {recording ? "Rec" : "Stopped"}
              </span>
            </div>
            <div className={SCREEN}>
              <Fields
                rows={[
                  ["History", settings?.historyEnabled === false ? "Off (turn it on in Settings)" : recording ? "Recording" : "Waiting for samples"],
                  ["Database", f.bytes(db?.fileBytes)],
                  ["Recent samples", f.num(db?.rawRows)],
                  ["Long-term samples", f.num(db?.minuteRows)],
                  ["Events kept", f.num(db?.events)],
                ]}
              />
              <ErrorNote error={runtime.data?.dbError ?? runtime.error} />
              <p className="mt-auto text-2xs text-faint">
                Monitor overhead: CPU {f.pct(frame?.selfUsage.cpuPct, 1)}, memory {f.bytes(frame?.selfUsage.workingSetBytes)}, {f.ms(frame?.selfUsage.tickCostMs)} per tick.
              </p>
            </div>
          </Module>
        </div>
      </div>
    </div>
  );
}
