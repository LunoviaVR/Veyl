import { FolderOpen } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNav } from "../App";
import { type Column, DataTable, type SortState, sortRows } from "../components/DataTable";
import { ContextCharts, EventRow, RawData } from "../components/EventViews";
import { Badge, Button, cn, Empty, ErrorNote, Fields, PageHeader, Panel, SearchInput, Select } from "../components/ui";
import * as f from "../../lib/format";
import { usePref, useQuery } from "../../lib/hooks";
import { api } from "../../lib/ipc";
import type { AppCrash, EventDetailView, StabilityEvent } from "../../lib/types";

const DAY = 86_400_000;
type Group = "none" | "app" | "exception" | "module";

function crashOf(e: StabilityEvent): Partial<AppCrash> & { hangType?: string | null } {
  if (e.detail.type === "appCrash") return e.detail;
  if (e.detail.type === "appHang") return { appName: e.detail.appName, appVersion: e.detail.appVersion, appPath: e.detail.appPath, processId: e.detail.processId, hangType: e.detail.hangType };
  return {};
}

const GENERIC = ["ntdll.dll", "kernelbase.dll", "kernel32.dll", "ucrtbase.dll", "msvcrt.dll", "vcruntime140.dll", "msvcp140.dll", "combase.dll", "user32.dll", "coreclr.dll"];

function Factors({ v }: { v: EventDetailView }) {
  const c = crashOf(v.event);
  const items: string[] = [];
  if (v.sameAppCount30d >= 3) items.push(`${c.appName} crashed ${v.sameAppCount30d} times in the 30 days up to this event — a recurring problem rather than a one-off.`);
  if (c.moduleName && v.sameModuleCount30d >= 2) items.push(`${v.sameModuleCount30d} crashes in this period faulted in ${c.moduleName}.`);
  if (c.moduleName && GENERIC.includes(c.moduleName.toLowerCase()))
    items.push(`${c.moduleName} is a core Windows or runtime library. It is usually where the error was detected, not where it originated; the defect is most often in the application or one of its components.`);
  if (c.moduleName && c.appName && c.moduleName.toLowerCase() !== c.appName.toLowerCase() && !GENERIC.includes(c.moduleName.toLowerCase()) && v.moduleFile?.company)
    items.push(`The faulting module comes from ${v.moduleFile.company}${v.moduleFile.product ? ` (${v.moduleFile.product})` : ""}. Updating or temporarily disabling that product is a reasonable test.`);
  if (c.exceptionCode === 0xc0000005) items.push("An access violation means the program used an invalid memory address. Software defects are the usual cause; unstable memory or CPU overclocks can produce the same code across unrelated programs.");
  if (c.exceptionCode === 0xc0000409) items.push("The program terminated itself after an internal integrity check failed (fail-fast). This is deliberate termination, normally a software defect or tampering detection.");
  if (c.exceptionCode === 0xc0000006) items.push("A code or data page could not be read from storage. Check the drive holding the program and the Storage health view.");
  if (v.related.some((r) => r.kind === "gpuDriverReset")) items.push("A graphics driver reset was recorded within 10 minutes of this crash.");
  if (v.related.some((r) => r.kind === "wheaError" || r.kind === "wheaCorrected")) items.push("A hardware error (WHEA) was recorded within 10 minutes of this crash.");
  if (items.length === 0) items.push("No additional pattern was found in the recorded events. A single crash on its own is not evidence of a system problem.");
  return (
    <ul className="list-disc pl-5 text-sm flex flex-col gap-1">
      {items.map((t) => (
        <li key={t}>{t}</li>
      ))}
    </ul>
  );
}

function CrashDetail({ e }: { e: StabilityEvent }) {
  const q = useQuery(() => api.eventDetail(e.channel, e.recordId), [e.channel, e.recordId]);
  const v = q.data;
  const c = crashOf(e);
  const wer = v?.linkedReport?.detail.type === "werReport" ? v.linkedReport.detail : null;
  return (
    <div className="flex flex-col gap-4">
      <ErrorNote error={q.error} />
      <Panel title="Summary">
        <Fields
          rows={[
            ["Application", `${c.appName ?? f.DASH} ${c.appVersion ? `(${c.appVersion})` : ""}`],
            ["Path", c.appPath ?? f.DASH],
            ["Time", f.dateTime(e.timeMs)],
            ["Kind", e.kind === "appHang" ? `Stopped responding${c.hangType ? ` (${c.hangType})` : ""} and was closed` : "Unhandled exception"],
            ["Process ID", f.num(c.processId)],
            ["Event", `${e.channel} · ${e.provider} ${e.eventId} · record ${e.recordId}`],
          ]}
        />
        {c.appPath && (
          <Button className="mt-2" onClick={() => api.revealPath(c.appPath!)}>
            <FolderOpen className="size-3.5" /> Open file location
          </Button>
        )}
      </Panel>
      {e.kind === "appCrash" && (
        <Panel title="Exception and faulting module">
          <Fields
            rows={[
              ["Exception code", c.exceptionCode != null ? `${f.hex(c.exceptionCode)} ${c.exceptionName ?? ""}` : f.DASH],
              ["Meaning", c.exceptionDescription ?? "Not in the built-in table; search the code in Microsoft's NTSTATUS reference."],
              ["Faulting module", `${c.moduleName ?? f.DASH} ${c.moduleVersion ? `(${c.moduleVersion})` : ""}`],
              ["Module path", c.modulePath ?? f.DASH],
              ["Module publisher", v?.moduleFile?.company ?? (v ? "Unknown or file no longer present" : "…")],
              ["Fault offset", c.faultOffset ?? f.DASH],
            ]}
          />
        </Panel>
      )}
      <Panel title="Windows Error Reporting">
        {wer ? (
          <Fields
            rows={[
              ["Report ID", c.reportId ?? f.DASH],
              ["Event name", wer.eventName ?? f.DASH],
              ["Bucket", wer.bucket ?? f.DASH],
              ["Report folder", wer.storePath?.replace(/^\\\\\?\\/, "") ?? "Not kept by Windows"],
              ["Attached files", wer.attachedFiles.length ? wer.attachedFiles.join("\n") : f.DASH],
            ]}
          />
        ) : (
          <p className="text-sm text-muted">{c.reportId ? `Report ${c.reportId}: no matching Windows Error Reporting event was found.` : "No report identifier was recorded for this event."} Crash dumps for applications are only written when local dump collection is configured; this application does not change that setting.</p>
        )}
      </Panel>
      <Panel title="Possible contributing factors">{v ? <Factors v={v} /> : <p className="text-muted">Loading…</p>}</Panel>
      <Panel title="System state around the crash (± 15 minutes)">{v ? <ContextCharts ctx={v.context} markerMs={e.timeMs} /> : <p className="text-muted">Loading…</p>}</Panel>
      <Panel title="Related Windows events (± 10 minutes)" bodyClass="p-0">
        {v?.related.length ? v.related.map((r) => <EventRow key={`${r.channel}${r.recordId}`} e={r} />) : <p className="p-3 text-muted text-sm">{v ? "None recorded." : "Loading…"}</p>}
      </Panel>
      <Panel title="Event message">
        <p className="text-xs whitespace-pre-wrap selectable font-mono">{v?.message ?? (v ? "Windows could not format this event's message." : "…")}</p>
        <div className="mt-2">
          <RawData data={e.data} />
        </div>
      </Panel>
      <Panel title="Suggested next steps">
        <ul className="list-disc pl-5 text-sm text-muted">
          <li>Update the application, and its runtime (Visual C++ Redistributable, .NET) if the faulting module belongs to it.</li>
          <li>If the faulting module belongs to another product (overlay, capture tool, antivirus), update or disable that product and test again.</li>
          <li>If many unrelated programs crash with access violations, test system stability: remove overclocks and run a memory test.</li>
        </ul>
      </Panel>
    </div>
  );
}

export default function Crashes() {
  const { params } = useNav();
  const [range, setRange] = usePref("crashRange", "90");
  const [group, setGroup] = usePref<Group>("crashGroup", "none");
  const [search, setSearch] = useState("");
  // The group row the user clicked; narrows the table to that group.
  const [groupFilter, setGroupFilter] = useState<string | null>(null);
  const [sort, setSort] = useState<SortState | null>({ id: "time", desc: true });
  const [selected, setSelected] = useState<string | null>(null);
  const now = Date.now();
  const q = useQuery(() => api.stabilityEvents(now - Number(range) * DAY, now + DAY, 20_000), [range], { events: ["stability-changed"] });
  const crashes = useMemo(() => (q.data ?? []).filter((e) => e.kind === "appCrash" || e.kind === "appHang"), [q.data]);

  useEffect(() => {
    if (params.record) setSelected(`${params.channel}:${params.record}`);
  }, [params.channel, params.record]);

  const groupKey = (e: StabilityEvent): string => {
    const c = crashOf(e);
    if (group === "app") return c.appName ?? "Unknown";
    if (group === "exception") return e.kind === "appHang" ? "Application hang" : c.exceptionCode != null ? `${f.hex(c.exceptionCode)} ${c.exceptionName ?? ""}` : "Unknown";
    if (group === "module") return e.kind === "appHang" ? "(hang)" : c.moduleName ?? "Unknown";
    return "";
  };

  const searched = crashes.filter((e) => {
    if (!search) return true;
    const c = crashOf(e);
    return `${c.appName} ${c.moduleName} ${c.exceptionName} ${c.appPath}`.toLowerCase().includes(search.toLowerCase());
  });
  const filtered = groupFilter == null ? searched : searched.filter((e) => groupKey(e) === groupFilter);

  const groups = useMemo(() => {
    if (group === "none") return [];
    const m = new Map<string, StabilityEvent[]>();
    for (const e of searched) m.set(groupKey(e), [...(m.get(groupKey(e)) ?? []), e]);
    return [...m.entries()].sort((a, b) => b[1].length - a[1].length);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [crashes, search, group]);

  const cols: Column<StabilityEvent>[] = [
    { id: "time", header: "Time", width: 190, sortValue: (e) => e.timeMs, cell: (e) => f.dateTime(e.timeMs) },
    { id: "app", header: "Application", width: "minmax(150px,1.2fr)", sortValue: (e) => crashOf(e).appName ?? null, cell: (e) => crashOf(e).appName ?? f.DASH },
    { id: "kind", header: "Type", width: 70, sortValue: (e) => e.kind, cell: (e) => (e.kind === "appHang" ? <Badge tone="info">Hang</Badge> : <Badge tone="bad">Crash</Badge>) },
    { id: "exc", header: "Exception", width: "minmax(160px,1fr)", sortValue: (e) => crashOf(e).exceptionCode ?? null, cell: (e) => (crashOf(e).exceptionCode != null ? `${f.hex(crashOf(e).exceptionCode)} ${crashOf(e).exceptionName ?? ""}` : f.DASH) },
    { id: "module", header: "Faulting module", width: "minmax(130px,1fr)", sortValue: (e) => crashOf(e).moduleName ?? null, cell: (e) => crashOf(e).moduleName ?? f.DASH },
    { id: "ver", header: "Version", width: 110, sortValue: (e) => crashOf(e).appVersion ?? null, cell: (e) => <span className="text-muted">{crashOf(e).appVersion ?? f.DASH}</span> },
  ];
  const sel = crashes.find((e) => `${e.channel}:${e.recordId}` === selected) ?? null;

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <PageHeader title="Crash Reports" subtitle="Application Error (1000) and Application Hang (1002) events, linked to Windows Error Reporting (1001)" />
      <div className="flex items-center gap-2 px-5 pb-3 shrink-0">
        <SearchInput placeholder="Search app, module, exception" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Search crashes" />
        <Select<Group>
          label="Group by"
          value={group}
          onChange={(g) => {
            setGroup(g);
            setGroupFilter(null);
          }}
          options={[
            { value: "none", label: "No grouping" },
            { value: "app", label: "Group by application" },
            { value: "exception", label: "Group by exception code" },
            { value: "module", label: "Group by faulting module" },
          ]}
        />
        <Select label="Range" value={range} onChange={setRange} options={[{ value: "7", label: "7 days" }, { value: "30", label: "30 days" }, { value: "90", label: "90 days" }, { value: "365", label: "1 year" }]} />
        <span className="ml-auto text-muted text-xs">{filtered.length} records</span>
      </div>
      <div className="px-5"><ErrorNote error={q.error} /></div>
      <div className="flex-1 min-h-0 flex gap-4 px-5 pb-5">
        <div className="w-[52%] min-w-[420px] flex flex-col gap-3 min-h-0">
          {group !== "none" && (
            <div className="crt max-h-56 overflow-y-auto shrink-0">
              {groups.map(([k, list]) => (
                <button
                  key={k}
                  aria-pressed={groupFilter === k}
                  onClick={() => setGroupFilter(groupFilter === k ? null : k)}
                  className={cn("w-full flex items-center gap-2 px-4 h-8 text-sm text-left border-b border-line/60 hover:bg-hover", groupFilter === k && "bg-accent/20 shadow-[inset_3px_0_0_var(--color-accent)]")}
                >
                  <span className="truncate">{k}</span>
                  {list.length >= 3 && <Badge tone="warn">Recurring</Badge>}
                  <span className="ml-auto tabular text-muted">{list.length}</span>
                </button>
              ))}
            </div>
          )}
          <div className="crt flex-1 min-h-0 overflow-hidden">
            {filtered.length ? (
              <DataTable label="Crashes" rows={sortRows(filtered, cols, sort)} columns={cols} rowKey={(e) => `${e.channel}:${e.recordId}`} sort={sort} onSort={setSort} selected={selected} onSelect={setSelected} />
            ) : (
              <Empty title={q.loading ? "Loading…" : "No application crashes recorded"}>Windows recorded no application crashes or hangs in this period.</Empty>
            )}
          </div>
        </div>
        <div className="flex-1 min-w-0 overflow-y-auto pr-1 pb-1">
          {sel ? (
            <CrashDetail e={sel} />
          ) : (
            <div className="crt">
              <Empty title="Select a crash">Details, faulting module, related events and recorded system state appear here.</Empty>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
