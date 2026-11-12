import { RotateCcw, Save, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useNav } from "../App";
import { type Column, DataTable } from "../components/DataTable";
import { Button, Empty, ErrorNote, PageHeader, Panel, Select, SeverityBadge, Switch, TextInput } from "../components/ui";
import * as f from "../../lib/format";
import { useQuery } from "../../lib/hooks";
import { api, errorText } from "../../lib/ipc";
import type { AlertRow, Severity, Settings, ThresholdRule } from "../../lib/types";
import { metricLabel } from "../../pages/History";

const SEVERITIES: { value: Severity; label: string }[] = [
  { value: "info", label: "Info" },
  { value: "warning", label: "Warning" },
  { value: "error", label: "Error" },
  { value: "critical", label: "Critical" },
];

function NumberField({ value, onChange, label, step = 1, width = "w-20" }: { value: number; onChange: (v: number) => void; label: string; step?: number; width?: string }) {
  return <TextInput type="number" aria-label={label} title={label} className={`${width} text-right tabular`} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} />;
}

function RuleRow({ r, onChange }: { r: ThresholdRule; onChange: (r: ThresholdRule) => void }) {
  return (
    <tr className="border-t border-line/60 align-middle">
      <td className="py-2 pr-3">
        <Switch label={`Enable ${r.name}`} checked={r.enabled} onChange={(v) => onChange({ ...r, enabled: v })} />
      </td>
      <td className="pr-3">
        <div className="font-medium">{r.name}</div>
        <div className="text-xs text-faint font-mono">{r.metric}</div>
      </td>
      <td className="pr-2 text-muted">{r.comparator === "above" ? ">" : "<"}</td>
      <td className="pr-2">
        <NumberField label="Threshold" value={r.threshold} step={0.5} onChange={(v) => onChange({ ...r, threshold: v })} />
      </td>
      <td className="pr-2">
        <NumberField label="Hysteresis" value={r.hysteresis} step={0.5} width="w-16" onChange={(v) => onChange({ ...r, hysteresis: v })} />
      </td>
      <td className="pr-2">
        <NumberField label="Trigger after (s)" value={r.durationSecs} onChange={(v) => onChange({ ...r, durationSecs: Math.max(0, Math.round(v)) })} />
      </td>
      <td className="pr-2">
        <NumberField label="Cooldown (s)" value={r.cooldownSecs} onChange={(v) => onChange({ ...r, cooldownSecs: Math.max(0, Math.round(v)) })} />
      </td>
      <td className="pr-2">
        <Select<Severity> label="Severity" value={r.severity} onChange={(v) => onChange({ ...r, severity: v })} options={SEVERITIES} />
      </td>
      <td>
        <Switch label={`Notify for ${r.name}`} checked={r.notify} onChange={(v) => onChange({ ...r, notify: v })} />
      </td>
    </tr>
  );
}

export default function Alerts() {
  const { settings, refreshSettings } = useNav();
  const active = useQuery(() => api.activeAlerts(), [], { intervalMs: 5000, events: ["alert"] });
  const history = useQuery(() => api.alertHistory(1000), [], { events: ["alert", "event-alert"], intervalMs: 30_000 });
  const [draft, setDraftState] = useState<Settings | null>(null);
  const [dirty, setDirtyState] = useState(false);
  const dirtyRef = useRef(false);
  const setDirty = (v: boolean) => {
    dirtyRef.current = v;
    setDirtyState(v);
  };
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const setDraft = (d: Settings) => {
    setDraftState(d);
    setDirty(true);
    setMsg(null);
  };
  // Follow settings changed elsewhere, but never overwrite unsaved edits.
  useEffect(() => {
    if (settings && !dirtyRef.current) setDraftState(settings);
  }, [settings]);

  const save = async () => {
    if (!draft || !settings) return;
    try {
      // Only this page's fields; everything else keeps its current stored value.
      await api.saveSettings({ ...settings, alertRules: draft.alertRules, eventAlerts: draft.eventAlerts, notificationsEnabled: draft.notificationsEnabled });
      setDirty(false);
      refreshSettings();
      setMsg("Alert settings saved.");
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const restoreDefaults = async () => {
    if (!confirm("Replace all threshold rules with the defaults? Your changes to the rules will be lost.")) return;
    try {
      await api.resetAlertRules();
      setDirty(false);
      refreshSettings();
      setMsg("Default rules restored.");
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const cols: Column<AlertRow>[] = [
    { id: "time", header: "Time", width: 190, cell: (a) => f.dateTime(a.timeMs) },
    { id: "sev", header: "Severity", width: 86, cell: (a) => <SeverityBadge severity={a.severity as Severity} /> },
    { id: "tr", header: "", width: 76, cell: (a) => <span className={a.transition === "fired" ? "text-warn" : "text-ok"}>{a.transition === "fired" ? "Triggered" : "Cleared"}</span> },
    { id: "name", header: "Alert", width: "minmax(180px,1.5fr)", cell: (a) => a.ruleName },
    {
      id: "measure",
      header: "Measurement",
      width: "minmax(220px,2fr)",
      cell: (a) => (a.ruleId.startsWith("event:") ? <span className="text-muted">{a.metric}</span> : <span className="text-muted">{metricLabel(a.metric)} = {f.num(a.value, 1)} (threshold {f.num(a.threshold, 1)}){a.transition === "fired" && a.sinceMs < a.timeMs ? ` · beyond threshold since ${f.timeOnly(a.sinceMs)}` : ""}</span>),
    },
  ];

  return (
    <div className="flex-1 overflow-y-auto">
      <PageHeader
        title="Alerts"
        subtitle="Threshold alerts with trigger duration, hysteresis and cooldown, plus alerts for new blue screens, unexpected restarts and repeated crashes"
        actions={
          <>
            <Button onClick={restoreDefaults}>
              <RotateCcw className="size-3.5" /> Restore defaults
            </Button>
            <Button variant="primary" onClick={save} disabled={!draft || !dirty}>
              <Save className="size-3.5" /> Save
            </Button>
          </>
        }
      />
      <div className="px-5 pb-6 flex flex-col gap-4">
        <ErrorNote error={error} />
        {msg && <p className="text-ok text-xs">{msg}</p>}
        <Panel
          title={`Active (${active.data?.length ?? 0})`}
          actions={
            !!active.data?.length && (
              <Button
                variant="ghost"
                title="Removes these from the active list. They stay in the history below."
                onClick={() => api.clearActiveAlerts().then(active.refresh, (e) => setError(errorText(e)))}
              >
                <X className="size-3.5" /> Clear
              </Button>
            )
          }
        >
          {active.data?.length ? (
            active.data.map((a) => (
              <div key={`${a.ruleId}${a.metric}`} className="flex items-center gap-3 text-sm py-1.5">
                <SeverityBadge severity={a.severity} />
                <span className="font-medium">{a.ruleName}</span>
                <span className="text-muted">
                  {metricLabel(a.metric)} = {f.num(a.value, 1)} (threshold {f.num(a.threshold, 1)}) since {f.timeOnly(a.sinceMs)}
                </span>
              </div>
            ))
          ) : (
            <p className="text-muted text-sm">No alert is active.</p>
          )}
        </Panel>
        {draft && (
          <Panel title="Threshold rules" bodyClass="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="label text-2xs text-muted">
                <tr>
                  <th className="text-left font-normal">On</th>
                  <th className="text-left font-normal">Rule</th>
                  <th />
                  <th className="text-left font-normal">Threshold</th>
                  <th className="text-left font-normal">Hysteresis</th>
                  <th className="text-left font-normal">After (s)</th>
                  <th className="text-left font-normal">Cooldown (s)</th>
                  <th className="text-left font-normal">Severity</th>
                  <th className="text-left font-normal">Notify</th>
                </tr>
              </thead>
              <tbody>
                {draft.alertRules.map((r, i) => (
                  <RuleRow key={r.id} r={r} onChange={(nr) => setDraft({ ...draft, alertRules: draft.alertRules.map((x, j) => (j === i ? nr : x)) })} />
                ))}
              </tbody>
            </table>
            <p className="text-xs text-faint mt-2">
              A rule triggers after the value stays beyond the threshold for the set time, clears only after it returns past threshold ∓ hysteresis, and cannot trigger again within the cooldown. Rules for sensors that are not available on this PC never trigger. Patterns with * apply to every matching device.
            </p>
          </Panel>
        )}
        {draft && (
          <Panel title="Event alerts">
            <div className="flex flex-col gap-2 text-sm">
              {(
                [
                  ["newBsod", "A new blue screen is recorded"],
                  ["unexpectedRestart", "Windows restarted unexpectedly (Kernel-Power 41)"],
                  ["repeatedCrashes", "The same application crashes repeatedly"],
                ] as const
              ).map(([k, label]) => (
                <label key={k} className="flex items-center gap-2">
                  <Switch label={label} checked={draft.eventAlerts[k]} onChange={(v) => setDraft({ ...draft, eventAlerts: { ...draft.eventAlerts, [k]: v } })} />
                  {label}
                </label>
              ))}
              <div className="flex items-center gap-2 pl-10 text-muted">
                <NumberField label="Crash count" value={draft.eventAlerts.crashCount} width="w-16" onChange={(v) => setDraft({ ...draft, eventAlerts: { ...draft.eventAlerts, crashCount: Math.round(v) } })} />
                crashes within
                <NumberField label="Window (minutes)" value={draft.eventAlerts.crashWindowMin} width="w-16" onChange={(v) => setDraft({ ...draft, eventAlerts: { ...draft.eventAlerts, crashWindowMin: Math.round(v) } })} />
                minutes
              </div>
              <label className="flex items-center gap-2">
                <Switch label="Windows notifications" checked={draft.notificationsEnabled} onChange={(v) => setDraft({ ...draft, notificationsEnabled: v })} />
                Show Windows notifications for alerts marked “Notify”
              </label>
            </div>
          </Panel>
        )}
        <Panel title="History" bodyClass="p-0">
          {history.data?.length ? (
            <div className="h-80">
              <DataTable label="Alert history" rows={history.data} columns={cols} rowKey={(a) => String(a.id)} />
            </div>
          ) : (
            <Empty title="No alerts recorded" />
          )}
        </Panel>
      </div>
    </div>
  );
}
