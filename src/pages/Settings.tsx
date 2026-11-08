import { FileDown, FolderOpen, Save, ShieldCheck } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useNav } from "../App";
import { Badge, Button, ErrorNote, Fields, PageHeader, Panel, Select, StatusNote, Switch, TextInput } from "../components/ui";
import * as f from "../lib/format";
import { useQuery } from "../lib/hooks";
import { api, errorText } from "../lib/ipc";
import { useLive } from "../lib/live";
import type { OverlaySettings, Settings as S } from "../lib/types";

function Row({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[minmax(200px,280px)_1fr] gap-4 py-2 border-b border-line/50 last:border-0 items-start">
      <div>
        <div className="text-[12.5px]">{label}</div>
        {hint && <div className="text-[11.5px] text-faint mt-0.5">{hint}</div>}
      </div>
      <div className="flex items-center gap-2 flex-wrap">{children}</div>
    </div>
  );
}

function Num({ value, onChange, label, min, max, step = 1 }: { value: number; onChange: (v: number) => void; label: string; min: number; max: number; step?: number }) {
  return <TextInput type="number" aria-label={label} className="w-24 text-right tabular" min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} />;
}

const OVERLAY_METRICS: { id: string; label: string }[] = [
  { id: "fps", label: "FPS" },
  { id: "frametime", label: "Frame time" },
  { id: "low1", label: "1% low" },
  { id: "gpuUtil", label: "GPU usage" },
  { id: "gpuTemp", label: "GPU temperature" },
  { id: "cpuUtil", label: "CPU usage" },
  { id: "cpuTemp", label: "CPU temperature" },
  { id: "vram", label: "VRAM" },
  { id: "ram", label: "RAM" },
];

export default function Settings() {
  const { settings, refreshSettings } = useNav();
  const status = useQuery(() => api.runtimeStatus(), [], { intervalMs: 10_000 });
  const self = useLive((s) => s.frame?.selfUsage);
  const upd = useQuery(() => api.updateStatus(), [], { events: ["update-available"] });
  const [d, setDraft] = useState<S | null>(null);
  // Stored settings the unsaved edits started from (null = no unsaved edits).
  const base = useRef<S | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [redact, setRedact] = useState(true);
  const [checking, setChecking] = useState(false);
  // Follow settings changed elsewhere (tray, Gaming, Alerts), but never overwrite unsaved edits.
  useEffect(() => {
    if (settings && !base.current) {
      setDraft(settings);
      setRedact(settings.exportRedactDefault);
    }
  }, [settings]);
  if (!d || !settings) return <PageHeader title="Settings" subtitle="Loading…" />;
  const setD = (next: S) => {
    base.current ??= settings;
    setDraft(next);
    setMsg(null);
  };
  const set = (patch: Partial<S>) => setD({ ...d, ...patch });
  const ov = (patch: Partial<OverlaySettings>) => setD({ ...d, overlay: { ...d.overlay, ...patch } });
  const save = async () => {
    try {
      // Sections not edited here keep their current stored value (they may have changed elsewhere meanwhile).
      const from = base.current ?? settings;
      const merged = Object.fromEntries(
        Object.keys(d).map((k) => {
          const key = k as keyof S;
          return [k, JSON.stringify(d[key]) === JSON.stringify(from[key]) ? settings[key] : d[key]];
        }),
      ) as unknown as S;
      const stored = await api.saveSettings(merged);
      base.current = null;
      setDraft(stored);
      refreshSettings();
      setMsg(JSON.stringify(stored) === JSON.stringify(merged) ? "Settings saved." : "Settings saved. Values outside the allowed range were adjusted.");
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  };
  const rs = status.data;

  return (
    <div className="flex-1 overflow-y-auto">
      <PageHeader
        title="Settings"
        actions={
          <Button variant="primary" onClick={save}>
            <Save className="size-3.5" /> Save
          </Button>
        }
      />
      <div className="px-5 pb-6 flex flex-col gap-3 max-w-[1100px]">
        <ErrorNote error={error} />
        {msg && <p className="text-ok text-[12px]">{msg}</p>}

        <Panel title="General">
          <Row label="Theme">
            <Select label="Theme" value={d.theme} onChange={(v) => set({ theme: v })} options={[{ value: "system", label: "Follow Windows" }, { value: "dark", label: "Dark" }, { value: "light", label: "Light" }]} />
          </Row>
          <Row label="Closing the window" hint="Keep monitoring and alerts running in the notification area.">
            <Switch label="Close to tray" checked={d.closeToTray} onChange={(v) => set({ closeToTray: v })} /> Keep running in the notification area
          </Row>
          <Row label="Minimizing the window" hint="Minimize hides the window to the notification area; click the tray icon to restore it.">
            <Switch label="Minimize to tray" checked={d.minimizeToTray} onChange={(v) => set({ minimizeToTray: v })} /> Minimize to the notification area
          </Row>
          <Row label="Start minimized">
            <Switch label="Start minimized" checked={d.startMinimized} onChange={(v) => set({ startMinimized: v })} />
          </Row>
        </Panel>

        <Panel title="Monitoring">
          <Row label="System sampling interval" hint="CPU, memory, disk, network, GPU and processes. 500–5000 ms.">
            <Num label="Sampling interval" value={d.sampleIntervalMs} min={500} max={5000} step={250} onChange={(v) => set({ sampleIntervalMs: v })} /> ms
          </Row>
          <Row label="Hardware sensor interval" hint="Thermal zones, CPU power, vendor GPU sensors. 1000–10000 ms.">
            <Num label="Sensor interval" value={d.sensorIntervalMs} min={1000} max={10000} step={500} onChange={(v) => set({ sensorIntervalMs: v })} /> ms
          </Row>
          <Row label="Drive health interval">
            <Num label="Drive health interval" value={d.storageHealthIntervalMin} min={5} max={1440} onChange={(v) => set({ storageHealthIntervalMin: v })} /> minutes
          </Row>
          <Row label="Stutter threshold" hint="A frame counts as a stutter when it takes longer than this multiple of the median frame time.">
            <Num label="Stutter factor" value={d.stutterFactor} min={1.5} max={10} step={0.1} onChange={(v) => set({ stutterFactor: v })} /> × median
          </Row>
        </Panel>

        <Panel title="History and retention">
          <Row label="Record history">
            <Switch label="Record history" checked={d.historyEnabled} onChange={(v) => set({ historyEnabled: v })} />
          </Row>
          <Row label="5-second data" hint="High-resolution recent telemetry.">
            <Num label="Raw retention hours" value={d.retention.rawHours} min={1} max={168} onChange={(v) => set({ retention: { ...d.retention, rawHours: v } })} /> hours
          </Row>
          <Row label="1-minute data" hint="Long-term telemetry.">
            <Num label="Minute retention days" value={d.retention.minuteDays} min={1} max={365} onChange={(v) => set({ retention: { ...d.retention, minuteDays: v } })} /> days
          </Row>
          <Row label="Events and alerts" hint="Copies of Windows events persist here even after Windows rotates its logs.">
            <Num label="Event retention days" value={d.retention.eventDays} min={7} max={3650} onChange={(v) => set({ retention: { ...d.retention, eventDays: v } })} /> days
          </Row>
          <Row label="Game sessions">
            <Num label="Session retention days" value={d.retention.sessionDays} min={1} max={3650} onChange={(v) => set({ retention: { ...d.retention, sessionDays: v } })} /> days
          </Row>
          {rs?.dbStats && (
            <Row label="Database">
              <span className="text-muted text-[12px] tabular">
                {f.bytes(rs.dbStats.fileBytes)} · {rs.dbStats.rawRows.toLocaleString()} recent + {rs.dbStats.minuteRows.toLocaleString()} long-term samples · {rs.dbStats.events.toLocaleString()} events · {rs.dbStats.sessions} sessions
              </span>
            </Row>
          )}
          {rs?.dbError && <ErrorNote error={rs.dbError} />}
        </Panel>

        <Panel title="Integrations">
          <Row label="PresentMon executable" hint="Intel PresentMon console application used for frame capture. Leave empty to use the tools folder.">
            <TextInput className="w-[420px] font-mono text-[11.5px]" placeholder="C:\Tools\PresentMon-2.x-x64.exe" value={d.presentmonPath ?? ""} onChange={(e) => set({ presentmonPath: e.target.value || null })} aria-label="PresentMon path" />
            <Button onClick={() => api.openAppFolder("tools")}>
              <FolderOpen className="size-3.5" /> Tools folder
            </Button>
          </Row>
          <Row label="Debugger (cdb.exe)" hint="From Debugging Tools for Windows (Windows SDK). Leave empty to detect the SDK location.">
            <TextInput className="w-[420px] font-mono text-[11.5px]" placeholder="C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\cdb.exe" value={d.debuggerPath ?? ""} onChange={(e) => set({ debuggerPath: e.target.value || null })} aria-label="Debugger path" />
          </Row>
          <Row
            label="Microsoft symbol server"
            hint="When enabled, the debugger downloads Windows symbol files from msdl.microsoft.com to resolve stack traces. Only symbol files are requested; dumps are never uploaded. Off by default."
          >
            <Switch label="Use Microsoft symbol server" checked={d.symbolServerEnabled} onChange={(v) => set({ symbolServerEnabled: v })} />
            {d.symbolServerEnabled && <Badge tone="warn">Network access when analyzing dumps</Badge>}
          </Row>
        </Panel>

        <Panel title="Game overlay">
          <Row label="Overlay" hint="A click-through window above other windows. Visible over borderless and windowed games; exclusive-fullscreen games draw over it. It never touches the game process.">
            <Switch label="Enable overlay" checked={d.overlay.enabled} onChange={(v) => ov({ enabled: v })} />
          </Row>
          <Row label="Position and layout">
            <Select label="Position" value={d.overlay.position} onChange={(v) => ov({ position: v })} options={[{ value: "topLeft", label: "Top left" }, { value: "topRight", label: "Top right" }, { value: "bottomLeft", label: "Bottom left" }, { value: "bottomRight", label: "Bottom right" }]} />
            <Select label="Layout" value={d.overlay.layout} onChange={(v) => ov({ layout: v })} options={[{ value: "vertical", label: "Vertical" }, { value: "horizontal", label: "Horizontal" }]} />
          </Row>
          <Row label="Scale / opacity / refresh">
            <Num label="Scale" value={d.overlay.scale} min={0.5} max={3} step={0.1} onChange={(v) => ov({ scale: v })} /> ×
            <Num label="Opacity" value={d.overlay.opacity} min={0.2} max={1} step={0.05} onChange={(v) => ov({ opacity: v })} />
            <Num label="Refresh interval" value={d.overlay.refreshMs} min={250} max={5000} step={250} onChange={(v) => ov({ refreshMs: v })} /> ms
          </Row>
          <Row label="Metrics">
            {OVERLAY_METRICS.map((m) => (
              <label key={m.id} className="flex items-center gap-1 text-[12.5px] mr-2">
                <input
                  type="checkbox"
                  checked={d.overlay.metrics.includes(m.id)}
                  onChange={(e) => ov({ metrics: e.target.checked ? [...d.overlay.metrics, m.id] : d.overlay.metrics.filter((x) => x !== m.id) })}
                />
                {m.label}
              </label>
            ))}
          </Row>
        </Panel>

        <Panel title="Updates">
          <Row label="Check for updates automatically" hint="Contacts GitHub Releases shortly after start and every 12 hours. Updates are signed; an update whose signature does not match is refused. Installing always asks you first.">
            <Switch label="Automatic update checks" checked={d.autoUpdateCheck} onChange={(v) => set({ autoUpdateCheck: v })} />
          </Row>
          <Row label="Status">
            <span className="text-[12.5px] text-muted">
              Version {upd.data?.currentVersion ?? f.DASH}
              {upd.data?.available ? ` · ${upd.data.available.version} available` : upd.data?.lastError ? " · last check failed" : upd.data?.lastCheckMs ? " · up to date" : ""}
              {upd.data?.lastCheckMs ? ` · checked ${f.relative(upd.data.lastCheckMs)}` : " · not checked yet"}
            </span>
            <Button
              disabled={checking}
              onClick={() => {
                setChecking(true);
                api
                  .checkForUpdate()
                  .then(() => upd.refresh())
                  .catch((e) => setError(errorText(e)))
                  .finally(() => setChecking(false));
              }}
            >
              {checking ? "Checking…" : "Check now"}
            </Button>
            {upd.data?.available && (
              <Button variant="primary" onClick={() => api.installUpdate().catch((e) => setError(errorText(e)))}>
                Install {upd.data.available.version} and restart
              </Button>
            )}
          </Row>
          {upd.data?.lastError && <p className="text-[12px] text-muted">Last check: {upd.data.lastError}</p>}
        </Panel>

        <Panel title="Privacy and export">
          <p className="text-[12.5px] text-muted mb-2">All data stays on this PC. The only network requests are update checks (when enabled above) and symbol downloads (when enabled). No telemetry is collected. Exports are written only to files you choose.</p>
          <Row label="Redact personal data by default" hint="Replaces your user name, profile path, computer name, account SIDs and drive serial numbers in exports.">
            <Switch label="Redact by default" checked={d.exportRedactDefault} onChange={(v) => set({ exportRedactDefault: v })} />
          </Row>
          <Row label="Diagnostic report" hint="JSON with system summary, findings, incidents, 30 days of events and drive health.">
            <label className="flex items-center gap-2 text-[12.5px]">
              <input type="checkbox" checked={redact} onChange={(e) => setRedact(e.target.checked)} /> Redact
            </label>
            <Button onClick={() => api.exportReport(redact).then((p) => p && setMsg(`Report saved to ${p}`)).catch((e) => setError(errorText(e)))}>
              <FileDown className="size-3.5" /> Export report…
            </Button>
          </Row>
        </Panel>

        <Panel title="About and permissions">
          <Fields
            rows={[
              ["Version", rs?.version ?? f.DASH],
              [
                "Privileges",
                <span className="flex items-center gap-2">
                  {rs?.elevated ? <Badge tone="warn">Administrator</Badge> : <Badge>Standard user</Badge>}
                  {!rs?.elevated && (
                    <Button onClick={() => api.restartAsAdmin().catch((e) => setError(errorText(e)))}>
                      <ShieldCheck className="size-3.5" /> Restart as administrator
                    </Button>
                  )}
                </span>,
              ],
              ["Needs administrator", "Frame capture without Performance Log Users membership, reading kernel dumps and live kernel reports, details of protected processes. Installed (release) builds start as administrator; Windows shows a UAC prompt at launch."],
              ["Data folder", <span className="font-mono text-[11.5px]">{rs?.dataDir}</span>],
              ["Logs", <span className="font-mono text-[11.5px]">{rs?.logDir}</span>],
              ["Monitor overhead", `CPU ${f.pct(self?.cpuPct, 2)} · memory ${f.bytes(self?.workingSetBytes)} · ${f.ms(self?.tickCostMs)} per sampling tick`],
              ["Dropped history batches", f.num(rs?.droppedWriteBatches)],
            ]}
          />
          <div className="flex gap-2 mt-3">
            <Button onClick={() => api.openAppFolder("data")}>
              <FolderOpen className="size-3.5" /> Data folder
            </Button>
            <Button onClick={() => api.openAppFolder("logs")}>
              <FolderOpen className="size-3.5" /> Logs
            </Button>
          </div>
          <div className="mt-3 flex flex-col gap-1">
            {rs?.eventChannels.map((c) => (
              <div key={c.channel} className="flex items-center gap-2 text-[12px]">
                <span className="w-24 text-muted">{c.channel} log</span>
                <StatusNote status={c.status} always />
              </div>
            ))}
          </div>
        </Panel>
      </div>
    </div>
  );
}
