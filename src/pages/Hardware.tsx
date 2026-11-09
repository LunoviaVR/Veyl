import { ChevronDown, ChevronRight, Download, RefreshCw, RotateCcw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNav } from "../App";
import { Badge, Button, cn, ErrorNote, Fields, Meter, PageHeader, Panel, SearchInput, StatusNote, Tabs } from "../components/ui";
import * as f from "../lib/format";
import { usePref, useQuery } from "../lib/hooks";
import { api, errorText } from "../lib/ipc";
import type { DriveHealth, SensorRow } from "../lib/types";

type Tab = "sensors" | "storage" | "system";

function SensorsTab() {
  const q = useQuery(() => api.sensors(), [], { intervalMs: 2000 });
  const [search, setSearch] = useState("");
  const [closed, setClosed] = useState<Set<string>>(new Set());
  const [install, setInstall] = useState<{ kind: "idle" } | { kind: "installing" } | { kind: "failed"; error: string }>({ kind: "idle" });
  const rows = q.data?.rows ?? [];
  // The install button goes on the first PawnIO provider that reports the driver missing.
  const pawnioMissing = q.data?.providers.find((p) => p.id.startsWith("pawnio-") && p.status.state === "providerUnavailable")?.id;
  const runInstall = () => {
    setInstall({ kind: "installing" });
    api.installPawnio().then(
      () => setInstall({ kind: "idle" }),
      (e) => setInstall({ kind: "failed", error: errorText(e) }),
    );
  };
  const groups = useMemo(() => {
    const s = search.toLowerCase();
    const m = new Map<string, SensorRow[]>();
    for (const r of rows) {
      if (s && !`${r.device} ${r.label}`.toLowerCase().includes(s)) continue;
      m.set(r.device, [...(m.get(r.device) ?? []), r]);
    }
    return [...m.entries()];
  }, [rows, search]);

  return (
    <div className="flex-1 min-h-0 flex flex-col px-5 pb-4 gap-3">
      <div className="flex flex-wrap gap-2 shrink-0">
        {q.data?.providers.map((p) => (
          <div key={p.id} className="border border-line rounded-md bg-panel px-3 py-2 min-w-64 flex-1">
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium text-[12.5px]">{p.name}</span>
              <Badge tone={p.status.state === "available" ? "ok" : "neutral"}>{p.readingCount} readings</Badge>
            </div>
            <div className="mt-1">
              <StatusNote status={p.status} always />
            </div>
            {p.id === pawnioMissing && (
              <div className="mt-2">
                <Button variant="primary" disabled={install.kind === "installing"} onClick={runInstall}>
                  <Download className="size-3.5" /> {install.kind === "installing" ? "Installing…" : "Install PawnIO"}
                </Button>
                <p className="mt-1 text-[12px] text-muted">Installs PawnIO 2.2.0, the open-source signed driver that comes with Veyl (pawnio.eu).</p>
                {install.kind === "failed" && <p className="mt-1 text-[12px] text-bad">{install.error}</p>}
              </div>
            )}
          </div>
        ))}
      </div>
      <div className="flex items-center gap-2 shrink-0">
        <SearchInput placeholder="Filter sensors" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Filter sensors" />
        <span className="text-[12px] text-muted">Min/max/avg since {f.timeOnly(q.data?.statsSinceMs)}</span>
        <Button className="ml-auto" onClick={() => api.resetSensorStats().then(q.refresh)}>
          <RotateCcw className="size-3.5" /> Reset min/max
        </Button>
      </div>
      <ErrorNote error={q.error} />
      <div className="flex-1 min-h-0 overflow-auto border border-line rounded-md bg-panel">
        <table className="w-full text-[12.5px] tabular">
          <thead className="sticky top-0 bg-panel text-muted text-[11.5px] border-b border-line">
            <tr>
              <th className="text-left font-normal px-3 h-7">Sensor</th>
              {["Current", "Minimum", "Maximum", "Average"].map((h) => (
                <th key={h} className="text-right font-normal px-3 w-28">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {groups.map(([device, list]) => (
              <SensorGroup key={device} device={device} rows={list} open={!closed.has(device)} toggle={() => {
                const n = new Set(closed);
                if (n.has(device)) n.delete(device);
                else n.add(device);
                setClosed(n);
              }} />
            ))}
          </tbody>
        </table>
        {groups.length === 0 && (
          <p className="p-4 text-muted text-[12.5px]">
            No sensor provider returned readings yet. Built-in providers: ACPI thermal zones, CPU power (Windows Energy Meter / RAPL), AMD GPUs (ADLX) and NVIDIA GPUs (NVML). CPU core temperatures and motherboard fans/voltages are added by the optional PawnIO driver, which comes with Veyl, when the app runs as administrator.
          </p>
        )}
      </div>
    </div>
  );
}

function SensorGroup({ device, rows, open, toggle }: { device: string; rows: SensorRow[]; open: boolean; toggle: () => void }) {
  return (
    <>
      <tr className="border-b border-line bg-raised/60">
        <td colSpan={5} className="px-2 h-7">
          <button onClick={toggle} className="flex items-center gap-1 font-medium" aria-expanded={open}>
            {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
            {device}
            <span className="text-faint font-normal ml-1">{rows[0]?.provider}</span>
          </button>
        </td>
      </tr>
      {open &&
        rows.map((r) => (
          <tr key={r.key} className="border-b border-line/50 hover:bg-hover">
            <td className="px-3 pl-8 h-6 truncate">{r.label}</td>
            <td className="px-3 text-right">{f.byUnit(r.value, r.unit)}</td>
            <td className="px-3 text-right text-muted">{f.byUnit(r.stats?.min, r.unit)}</td>
            <td className="px-3 text-right text-muted">{f.byUnit(r.stats?.max, r.unit)}</td>
            <td className="px-3 text-right text-muted">{f.byUnit(r.stats?.avg, r.unit)}</td>
          </tr>
        ))}
    </>
  );
}

function DriveCard({ d }: { d: DriveHealth }) {
  const n = d.nvme;
  const warnFlags = n ? n.criticalWarning : 0;
  return (
    <Panel
      title={<span className="normal-case tracking-normal text-fg text-[12.5px]">{d.model ?? `Disk ${d.index}`}</span>}
      actions={
        <>
          {d.busType && <Badge>{d.busType}</Badge>}
          {n && (warnFlags ? <Badge tone="bad">Critical warning</Badge> : <Badge tone="ok">No critical warnings</Badge>)}
        </>
      }
    >
      <Fields
        rows={[
          ["Disk", `PhysicalDrive${d.index}`],
          ["Capacity", f.bytes(d.sizeBytes)],
          ["Firmware", d.firmware ?? f.DASH],
          ["Temperature", d.temperatureC != null ? `${f.temp(d.temperatureC)}${d.temperatureWarningC != null ? ` (warning at ${f.temp(d.temperatureWarningC)})` : ""}` : "Not reported"],
          ...(n
            ? ([
                ["Endurance used", <span className="flex items-center gap-2 w-56"><span className="w-10">{n.percentageUsed}%</span><Meter value={n.percentageUsed} /></span>],
                ["Available spare", `${n.availableSparePct}% (threshold ${n.availableSpareThresholdPct}%)`],
                ["Power-on hours", f.num(n.powerOnHours)],
                ["Power cycles / unsafe shutdowns", `${f.num(n.powerCycles)} / ${f.num(n.unsafeShutdowns)}`],
                ["Data written", f.bytes(n.dataUnitsWritten * 512_000)],
                ["Data read", f.bytes(n.dataUnitsRead * 512_000)],
                ["Media / integrity errors", <span className={n.mediaErrors > 0 ? "text-warn" : ""}>{f.num(n.mediaErrors)}</span>],
                ["Error log entries", f.num(n.errorLogEntries)],
                ["Time above warning / critical temp", `${n.warningTempMinutes} / ${n.criticalTempMinutes} min`],
              ] as [string, React.ReactNode][])
            : []),
        ]}
      />
      <div className="mt-2 flex flex-col gap-1">
        <StatusNote status={d.status} />
        <StatusNote status={d.healthStatus} />
      </div>
    </Panel>
  );
}

function StorageTab() {
  const q = useQuery(() => api.driveHealth(), [], { intervalMs: 15_000 });
  return (
    <div className="flex-1 overflow-y-auto px-5 pb-4 flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <span className="text-[12px] text-muted">Read from each drive through the Windows storage query interface. Updated {f.relative(q.data?.updatedMs)}.</span>
        <Button className="ml-auto" onClick={() => api.refreshDriveHealth().then(() => setTimeout(q.refresh, 1500))}>
          <RefreshCw className="size-3.5" /> Refresh
        </Button>
      </div>
      <ErrorNote error={q.error} />
      <div className="grid grid-cols-1 xl:grid-cols-2 gap-3">
        {q.data?.drives.map((d) => <DriveCard key={d.index} d={d} />)}
      </div>
      {q.data && q.data.drives.length === 0 && <p className="text-muted">Drive information has not been read yet.</p>}
    </div>
  );
}

function SystemTab() {
  const q = useQuery(() => api.systemInfo(), []);
  const s = q.data;
  return (
    <div className="flex-1 overflow-y-auto px-5 pb-4 grid grid-cols-1 xl:grid-cols-2 gap-3 content-start">
      <Panel title="Processor">
        <Fields
          rows={[
            ["Model", s?.cpu.brand ?? f.DASH],
            ["Vendor", s?.cpu.vendor ?? f.DASH],
            ["Packages", f.num(s?.cpu.packages)],
            ["Physical cores", f.num(s?.cpu.physicalCores)],
            ["Logical processors", f.num(s?.cpu.logicalProcessors)],
            ["Base clock", s?.cpu.baseMhz ? f.mhz(s.cpu.baseMhz) : f.DASH],
            ["Hybrid layout", s?.cpu.performanceCores ? `${s.cpu.performanceCores} performance + ${s.cpu.efficiencyCores} efficiency cores` : "Not hybrid / not reported"],
          ]}
        />
      </Panel>
      <Panel title="System">
        <Fields
          rows={[
            ["Computer", s?.computerName ?? f.DASH],
            ["Windows", s?.osName ?? f.DASH],
            ["Build", s?.osBuild ?? f.DASH],
            ["Installed memory (usable)", f.bytes(s?.installedMemoryBytes)],
            ["Booted", f.dateTime(s?.bootTimeMs)],
            ["Running as administrator", s ? (s.elevated ? "Yes" : "No") : f.DASH],
          ]}
        />
      </Panel>
      {s?.gpus.map((g) => (
        <Panel key={g.luid} title="Graphics adapter">
          <Fields
            rows={[
              ["Name", g.name],
              ["Vendor / device ID", `${f.hex(g.vendorId, 4)} / ${f.hex(g.deviceId, 4)}`],
              ["Dedicated memory", f.bytes(g.dedicatedMemoryBytes)],
              ["Shared memory", f.bytes(g.sharedMemoryBytes)],
              ["Adapter LUID", <span className="font-mono text-[11.5px]">{g.luid}</span>],
            ]}
          />
        </Panel>
      ))}
    </div>
  );
}

export default function Hardware() {
  const { params } = useNav();
  const [tab, setTab] = usePref<Tab>("hwTab", "sensors");
  useEffect(() => {
    if (params.tab) setTab(params.tab as Tab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params.tab]);
  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <PageHeader title="Hardware" subtitle="Sensors from Windows and optional providers, drive health, and hardware identification" />
      <Tabs value={tab} onChange={setTab} tabs={[{ value: "sensors", label: "Sensors" }, { value: "storage", label: "Storage health" }, { value: "system", label: "System" }]} />
      <div className={cn("flex-1 min-h-0 flex flex-col pt-3")}>
        {tab === "sensors" && <SensorsTab />}
        {tab === "storage" && <StorageTab />}
        {tab === "system" && <SystemTab />}
      </div>
    </div>
  );
}
