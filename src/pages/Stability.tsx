import { CheckCircle2, Download } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNav } from "../App";
import { ContextCharts, EventRow, KIND_LABEL, RawData } from "../components/EventViews";
import { Button, cn, Empty, ErrorNote, Modal, PageHeader, Panel, Select, SeverityBadge, StatusNote } from "../components/ui";
import * as f from "../lib/format";
import { usePref, useQuery } from "../lib/hooks";
import { api, errorText } from "../lib/ipc";
import type { EventKind, Finding, StabilityEvent } from "../lib/types";

const DAY = 86_400_000;
/** Days since the epoch of the local calendar date (DST-safe: uses that instant's offset). */
const localDay = (ms: number) => Math.floor((ms - new Date(ms).getTimezoneOffset() * 60_000) / DAY);
const RANGES = [
  { value: "7", label: "Last 7 days" },
  { value: "30", label: "Last 30 days" },
  { value: "90", label: "Last 90 days" },
  { value: "365", label: "Last year" },
];
const FILTERS: { value: string; label: string; kinds: EventKind[] | null }[] = [
  { value: "failures", label: "Failures and warnings", kinds: ["appCrash", "appHang", "bugcheck", "kernelPowerUnexpected", "unexpectedShutdown", "wheaError", "wheaCorrected", "gpuDriverReset", "storageError", "serviceFailure"] },
  { value: "all", label: "All recorded events", kinds: null },
  { value: "system", label: "Shutdowns and blue screens", kinds: ["bugcheck", "kernelPowerUnexpected", "unexpectedShutdown", "systemStart", "systemShutdown"] },
  { value: "hardware", label: "Hardware, driver and storage", kinds: ["wheaError", "wheaCorrected", "gpuDriverReset", "storageError"] },
  { value: "apps", label: "Applications", kinds: ["appCrash", "appHang", "werReport"] },
];

function FindingCard({ x, highlight }: { x: Finding; highlight: boolean }) {
  // Summary lines have no time and are not counted in evidenceTotal.
  const shownEvents = x.evidence.filter((e) => e.timeMs != null).length;
  return (
    <div className={cn("border rounded-md bg-panel p-3 flex flex-col gap-2", highlight ? "border-accent" : "border-line")}>
      <div className="flex items-start gap-2">
        <SeverityBadge severity={x.severity} />
        <div className="min-w-0">
          <div className="font-medium">{x.title}</div>
          <div className="text-muted text-[12.5px] mt-0.5">{x.explanation}</div>
        </div>
      </div>
      {x.caveat && <p className="text-[12px] text-faint italic">{x.caveat}</p>}
      <details className="text-[12px]">
        <summary className="cursor-pointer text-muted">
          Evidence ({x.evidenceTotal}){x.firstMs != null && ` · ${f.dateTime(x.firstMs)} – ${f.dateTime(x.lastMs)}`}
        </summary>
        <ul className="mt-1 flex flex-col gap-0.5">
          {x.evidence.map((e, i) => (
            <li key={i} className="flex gap-2">
              <span className="text-faint tabular w-36 shrink-0">{e.timeMs != null ? f.dateTime(e.timeMs) : ""}</span>
              <span className="selectable">{e.text}</span>
            </li>
          ))}
          {x.evidenceTotal > shownEvents && <li className="text-faint">… and {x.evidenceTotal - shownEvents} more</li>}
        </ul>
      </details>
      {x.suggestions.length > 0 && (
        <ul className="text-[12px] list-disc pl-5 text-muted">
          {x.suggestions.map((s) => (
            <li key={s}>{s}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

function AroundModal({ e, onClose }: { e: StabilityEvent; onClose: () => void }) {
  const { go } = useNav();
  const [half, setHalf] = useState("900000");
  const q = useQuery(() => api.timeline(e.timeMs, Number(half)), [e.timeMs, half]);
  const others = q.data?.events.filter((x) => !(x.channel === e.channel && x.recordId === e.recordId));
  return (
    <Modal open onOpenChange={(o) => !o && onClose()} title={`Around ${f.dateTime(e.timeMs)}`} wide>
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-2">
          <SeverityBadge severity={e.severity} />
          <span className="font-medium">{e.title}</span>
          <Select
            className="ml-auto"
            label="Window"
            value={half}
            onChange={setHalf}
            options={[
              { value: "300000", label: "± 5 minutes" },
              { value: "900000", label: "± 15 minutes" },
              { value: "3600000", label: "± 1 hour" },
              { value: "21600000", label: "± 6 hours" },
            ]}
          />
        </div>
        <RawData data={e.data} />
        <ErrorNote error={q.error} />
        {q.data && <ContextCharts ctx={q.data.context} markerMs={e.timeMs} />}
        <div>
          <h3 className="text-[12px] text-muted mb-1">Other Windows events in this window</h3>
          <div className="border border-line rounded-md max-h-64 overflow-y-auto">
            {others?.map((x) => <EventRow key={`${x.channel}${x.recordId}`} e={x} />)}
            {others?.length === 0 && <p className="p-3 text-muted text-[12.5px]">None recorded.</p>}
          </div>
        </div>
        {(e.kind === "appCrash" || e.kind === "appHang") && <Button onClick={() => go("crashes", { channel: e.channel, record: e.recordId })}>Open crash details</Button>}
        {(e.kind === "bugcheck" || e.kind === "kernelPowerUnexpected") && <Button onClick={() => go("bsod")}>Open Blue Screen Analyzer</Button>}
      </div>
    </Modal>
  );
}

export default function Stability() {
  const { params } = useNav();
  const [range, setRange] = usePref("stabRange", "30");
  const [filter, setFilter] = usePref("stabFilter", "failures");
  const [open, setOpen] = useState<StabilityEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const now = Date.now();
  const days = Number(range);
  const events = useQuery(() => api.stabilityEvents(now - days * DAY, now + DAY, 20_000), [range], { events: ["stability-changed"] });
  const findings = useQuery(() => api.diagnostics(), [], { events: ["stability-changed"] });
  const channels = useQuery(() => api.eventChannels(), [], { events: ["stability-changed"] });

  useEffect(() => {
    if (params.finding) document.getElementById(`finding-${params.finding}`)?.scrollIntoView({ block: "center" });
  }, [params.finding, findings.data]);

  const kinds = FILTERS.find((x) => x.value === filter)?.kinds ?? null;
  const list = useMemo(() => (events.data ?? []).filter((e) => !kinds || kinds.includes(e.kind)), [events.data, kinds]);

  // Daily counts for the stability strip, by local calendar day.
  const daily = useMemo(() => {
    const buckets = new Map<number, { crash: number; system: number; hw: number }>();
    for (const e of events.data ?? []) {
      const d = localDay(e.timeMs);
      const b = buckets.get(d) ?? { crash: 0, system: 0, hw: 0 };
      if (e.kind === "appCrash" || e.kind === "appHang") b.crash++;
      else if (["bugcheck", "kernelPowerUnexpected", "unexpectedShutdown"].includes(e.kind)) b.system++;
      else if (["wheaError", "wheaCorrected", "gpuDriverReset", "storageError"].includes(e.kind)) b.hw++;
      buckets.set(d, b);
    }
    const today = localDay(now);
    return Array.from({ length: Math.min(days, 90) }, (_, i) => {
      const d = today - (Math.min(days, 90) - 1 - i);
      return { d, ...(buckets.get(d) ?? { crash: 0, system: 0, hw: 0 }) };
    });
  }, [events.data, days, now]);
  const maxDaily = Math.max(1, ...daily.map((d) => d.crash + d.system + d.hw));

  return (
    <div className="flex-1 overflow-y-auto">
      <PageHeader
        title="Stability"
        subtitle="Windows reliability events, unexpected shutdowns and hardware errors, with diagnostic findings drawn from recorded evidence"
        actions={
          <Button onClick={() => api.exportEventsCsv(now - days * DAY, now + DAY, true).catch((e) => setError(errorText(e)))}>
            <Download className="size-3.5" /> Export CSV
          </Button>
        }
      />
      <div className="px-5 pb-5 flex flex-col gap-3">
        <ErrorNote error={error ?? events.error} />
        {channels.data?.map((c) => <StatusNote key={c.channel} status={c.status} />)}

        <Panel title="Findings">
          <div className="flex flex-col gap-2">
            {(findings.data ?? []).map((x) => (
              <div id={`finding-${x.id}`} key={x.id}>
                <FindingCard x={x} highlight={params.finding === x.id} />
              </div>
            ))}
            {findings.data?.length === 0 && (
              <Empty icon={<CheckCircle2 className="size-6 text-ok" />} title="No recurring problems found">
                The diagnostic rules found no repeated crashes, unexpected restarts, hardware errors, storage warnings, sustained memory pressure or thermal saturation in the recorded data.
              </Empty>
            )}
            <ErrorNote error={findings.error} />
          </div>
        </Panel>

        <Panel
          title="Daily events"
          actions={
            <span className="flex gap-3 text-[11.5px] text-muted">
              <span className="flex items-center gap-1"><span className="size-2 rounded-sm bg-warn" />Applications</span>
              <span className="flex items-center gap-1"><span className="size-2 rounded-sm bg-bad" />Shutdowns / blue screens</span>
              <span className="flex items-center gap-1"><span className="size-2 rounded-sm bg-chart-4" />Hardware / drivers</span>
            </span>
          }
        >
          <div className="flex items-end gap-[2px] h-20" role="img" aria-label="Events per day">
            {daily.map((d) => {
              const total = d.crash + d.system + d.hw;
              return (
                <div key={d.d} className="flex-1 flex flex-col justify-end h-full" title={`${new Date(d.d * DAY).toLocaleDateString(undefined, { timeZone: "UTC" })}:${d.crash} app, ${d.system} system, ${d.hw} hardware`}>
                  {total === 0 ? (
                    <div className="h-px bg-line-strong" />
                  ) : (
                    <>
                      <div className="bg-chart-4" style={{ height: `${(d.hw / maxDaily) * 100}%` }} />
                      <div className="bg-bad" style={{ height: `${(d.system / maxDaily) * 100}%` }} />
                      <div className="bg-warn" style={{ height: `${(d.crash / maxDaily) * 100}%` }} />
                    </>
                  )}
                </div>
              );
            })}
          </div>
        </Panel>

        <Panel
          title={`Timeline · ${list.length} events`}
          actions={
            <>
              <Select label="Filter" value={filter} onChange={setFilter} options={FILTERS.map((x) => ({ value: x.value, label: x.label }))} />
              <Select label="Range" value={range} onChange={setRange} options={RANGES} />
            </>
          }
          bodyClass="p-0"
        >
          <div className="max-h-[480px] overflow-y-auto">
            {list.slice(0, 2000).map((e) => (
              <EventRow key={`${e.channel}${e.recordId}`} e={e} onOpen={() => setOpen(e)} />
            ))}
            {list.length === 0 && !events.loading && <Empty title="No events in this range">Windows recorded no events of the selected kinds.</Empty>}
          </div>
          <p className="px-3 py-2 text-[11.5px] text-faint border-t border-line">
            Kinds: {Object.values(KIND_LABEL).join(", ")}. Select an event to see the telemetry and other events recorded around it.
          </p>
        </Panel>
      </div>
      {open && <AroundModal e={open} onClose={() => setOpen(null)} />}
    </div>
  );
}
