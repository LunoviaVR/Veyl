import * as CM from "@radix-ui/react-context-menu";
import { ChevronDown, ChevronRight, Copy, FolderOpen, Info, ListTree, PauseCircle, Skull, Gauge, XCircle } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useNav } from "../App";
import { type Column, DataTable, type SortState, sortRows } from "../components/DataTable";
import { Badge, Button, cn, ErrorNote, Fields, Modal, PageHeader, Panel, SearchInput, StatusNote, Tabs } from "../components/ui";
import * as f from "../../lib/format";
import { usePref, useQuery } from "../../lib/hooks";
import { api, errorText } from "../../lib/ipc";
import type { ConnectionRow, Priority, ProcessRow, ServiceInfo, StartupItem } from "../../lib/types";
import { buildTree } from "../../pages/Processes";

type Tab = "processes" | "services" | "startup" | "connections";
const key = (r: ProcessRow) => `${r.pid}:${r.startTimeMs ?? "x"}`;
/** Each screen starts with its busiest process on top. */
const DEFAULT_SORT: SortState = { id: "cpu", desc: true };

const PRIORITIES: { value: Priority; label: string }[] = [
  { value: "idle", label: "Low" },
  { value: "belowNormal", label: "Below normal" },
  { value: "normal", label: "Normal" },
  { value: "aboveNormal", label: "Above normal" },
  { value: "high", label: "High" },
];

type Node = { row: ProcessRow; depth: number; hasChildren: boolean };

/** What a row's menu can do. The dialogs these open belong to the tab, so both screens share them. */
interface RowActions {
  end: (row: ProcessRow, tree: boolean) => void;
  details: (row: ProcessRow) => void;
  run: (fn: () => Promise<unknown>, ok: string) => void;
}

function RowMenu({ row: r, actions }: { row: ProcessRow; actions: RowActions }) {
  const { go } = useNav();
  return (
    <CM.Portal>
      <CM.Content className="crt min-w-60 p-1 text-sm z-50">
        <MenuItem icon={<XCircle className="size-3.5" />} onSelect={() => actions.end(r, false)}>
          End task
        </MenuItem>
        <MenuItem icon={<Skull className="size-3.5" />} onSelect={() => actions.end(r, true)}>
          End process tree
        </MenuItem>
        <CM.Sub>
          <CM.SubTrigger className="flex items-center gap-2 h-8 px-2.5 rounded-[3px] outline-none data-[highlighted]:bg-hover">
            <Gauge className="size-3.5 text-muted" /> Set priority <ChevronRight className="size-3 ml-auto" />
          </CM.SubTrigger>
          <CM.Portal>
            <CM.SubContent className="crt min-w-44 p-1 text-sm z-50">
              {PRIORITIES.map((p) => (
                <MenuItem key={p.value} onSelect={() => actions.run(() => api.setPriority(r.pid, r.name, r.startTimeMs, p.value), `Priority of ${r.name} set to ${p.label}`)}>
                  {p.label}
                </MenuItem>
              ))}
            </CM.SubContent>
          </CM.Portal>
        </CM.Sub>
        <CM.Separator className="h-px bg-line my-1" />
        <MenuItem icon={<FolderOpen className="size-3.5" />} disabled={!r.path} onSelect={() => r.path && actions.run(() => api.revealPath(r.path!), "Opened file location")}>
          Open file location
        </MenuItem>
        <MenuItem icon={<Info className="size-3.5" />} onSelect={() => actions.details(r)}>
          Properties
        </MenuItem>
        <MenuItem icon={<Copy className="size-3.5" />} onSelect={() => navigator.clipboard.writeText(String(r.pid))}>
          Copy PID
        </MenuItem>
        {r.path && (
          <MenuItem icon={<Copy className="size-3.5" />} onSelect={() => navigator.clipboard.writeText(r.path!)}>
            Copy path
          </MenuItem>
        )}
        <MenuItem icon={<ListTree className="size-3.5" />} onSelect={() => go("gaming", { pid: r.pid })}>
          Capture frame rate…
        </MenuItem>
      </CM.Content>
    </CM.Portal>
  );
}

/**
 * One screen of processes with its own filter, sort order and row menu.
 * `tree` nests children under their parents until a filter is typed.
 */
function ProcessScreen({
  id,
  title,
  noun,
  rows,
  tree = false,
  loading,
  selected,
  onSelect,
  actions,
  className,
}: {
  /** Names the saved sort order. */
  id: string;
  title: string;
  noun: string;
  rows: ProcessRow[];
  tree?: boolean;
  loading: boolean;
  selected: string | null;
  onSelect: (key: string) => void;
  actions: RowActions;
  className?: string;
}) {
  const [search, setSearch] = useState("");
  const [sort, setSort] = usePref<SortState>(`procSort.${id}`, DEFAULT_SORT);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  // The row that was right-clicked; null elsewhere (header, empty space) so no stale menu opens.
  const [menuRow, setMenuRow] = useState<ProcessRow | null>(null);

  const columns: Column<Node>[] = useMemo(
    () => [
      {
        id: "name",
        header: "Name",
        width: "minmax(220px,2fr)",
        sortValue: (x) => x.row.name.toLowerCase(),
        cell: (x) => (
          <span className="flex items-center gap-1 min-w-0">
            {tree &&
              (x.hasChildren ? (
                <button
                  className="text-faint hover:text-fg"
                  aria-label={collapsed.has(key(x.row)) ? "Expand" : "Collapse"}
                  onClick={(e) => {
                    e.stopPropagation();
                    const n = new Set(collapsed);
                    if (n.has(key(x.row))) n.delete(key(x.row));
                    else n.add(key(x.row));
                    setCollapsed(n);
                  }}
                >
                  {collapsed.has(key(x.row)) ? <ChevronRight className="size-3.5" /> : <ChevronDown className="size-3.5" />}
                </button>
              ) : (
                <span className="w-3.5 shrink-0" />
              ))}
            <span className="truncate">{x.row.name || `PID ${x.row.pid}`}</span>
            {x.row.suspended && <PauseCircle className="size-3 text-info shrink-0" aria-label="Suspended" />}
          </span>
        ),
      },
      { id: "pid", header: "PID", width: 64, align: "right", sortValue: (x) => x.row.pid, cell: (x) => x.row.pid },
      { id: "ppid", header: "Parent", width: 64, align: "right", sortValue: (x) => x.row.parentPid, cell: (x) => x.row.parentPid },
      { id: "status", header: "Status", width: 78, sortValue: (x) => (x.row.suspended ? 1 : 0), cell: (x) => (x.row.suspended ? <span className="text-info">Suspended</span> : <span className="text-muted">Running</span>) },
      { id: "cpu", header: "CPU", width: 64, align: "right", sortValue: (x) => x.row.cpuPct, cell: (x) => f.pct(x.row.cpuPct, 1), bar: (x) => (x.row.pid === 0 ? null : (x.row.cpuPct ?? 0) / 100), title: "Share of total CPU time across all logical processors" },
      { id: "mem", header: "Memory", width: 86, align: "right", sortValue: (x) => x.row.privateWorkingSetBytes, cell: (x) => f.bytes(x.row.privateWorkingSetBytes), title: "Private working set" },
      { id: "commit", header: "Commit", width: 86, align: "right", sortValue: (x) => x.row.privateBytes, cell: (x) => f.bytes(x.row.privateBytes), title: "Private committed bytes" },
      {
        id: "io",
        header: "I/O",
        width: 86,
        align: "right",
        sortValue: (x) => (x.row.ioReadBps == null ? null : x.row.ioReadBps + (x.row.ioWriteBps ?? 0)),
        cell: (x) => (x.row.ioReadBps == null ? f.DASH : f.rate(x.row.ioReadBps + (x.row.ioWriteBps ?? 0))),
        title: "All I/O (file, network, device) — read + write",
      },
      { id: "gpu", header: "GPU", width: 58, align: "right", sortValue: (x) => x.row.gpuPct, cell: (x) => f.pct(x.row.gpuPct, 0), bar: (x) => (x.row.gpuPct ?? 0) / 100, title: "Busiest GPU engine used by the process" },
      { id: "vram", header: "GPU mem", width: 82, align: "right", sortValue: (x) => x.row.gpuDedicatedBytes, cell: (x) => f.bytes(x.row.gpuDedicatedBytes) },
      { id: "threads", header: "Threads", width: 64, align: "right", sortValue: (x) => x.row.threads, cell: (x) => x.row.threads },
      { id: "handles", header: "Handles", width: 70, align: "right", sortValue: (x) => x.row.handles, cell: (x) => x.row.handles },
      { id: "start", header: "Started", width: 124, align: "right", sortValue: (x) => x.row.startTimeMs, cell: (x) => f.whenCompact(x.row.startTimeMs) },
      { id: "path", header: "Path", width: "minmax(200px,2fr)", sortValue: (x) => x.row.path ?? null, cell: (x) => <span className="text-muted" title={x.row.path ?? "Not accessible"}>{x.row.path ?? f.DASH}</span> },
    ],
    [tree, collapsed],
  );

  const query = search.trim().toLowerCase();
  const view = useMemo(() => {
    const flat = (list: ProcessRow[]) => sortRows(list.map((row): Node => ({ row, depth: 0, hasChildren: false })), columns, sort);
    if (tree && !query) return buildTree(rows, collapsed, (list) => flat(list).map((x) => x.row));
    return flat(query ? rows.filter((r) => r.name.toLowerCase().includes(query) || String(r.pid) === query || r.path?.toLowerCase().includes(query)) : rows);
  }, [rows, query, tree, sort, collapsed, columns]);

  return (
    <Panel
      className={className}
      title={`${title} · ${query ? `${view.length} of ${rows.length}` : rows.length}`}
      actions={<SearchInput placeholder={`Filter ${noun}`} title="By name, PID or path" value={search} onChange={(e) => setSearch(e.target.value)} aria-label={`Filter ${noun}`} />}
      bodyClass="p-0 overflow-hidden"
    >
      <CM.Root onOpenChange={(o) => !o && setMenuRow(null)}>
        <CM.Trigger asChild>
          <div className="h-full">
            <DataTable
              label={title}
              rows={view}
              columns={columns}
              rowKey={(x) => key(x.row)}
              sort={sort}
              onSort={setSort}
              selected={selected}
              onSelect={onSelect}
              onActivate={(x) => actions.details(x.row)}
              onContextMenu={(x) => setMenuRow(x.row)}
              indent={tree && !query ? (x) => x.depth : undefined}
              empty={query ? `No ${noun} match the filter` : loading ? "Loading processes…" : `No ${noun}`}
            />
          </div>
        </CM.Trigger>
        {menuRow && <RowMenu row={menuRow} actions={actions} />}
      </CM.Root>
    </Panel>
  );
}

function ProcessesTab() {
  const table = useQuery(() => api.processes(), [], { intervalMs: 1500 });
  const [treeMode, setTreeMode] = usePref("procTree", false);
  // One selection across both screens: End task acts on it wherever it is.
  const [selected, setSelected] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{ row: ProcessRow; tree: boolean } | null>(null);
  const [details, setDetails] = useState<ProcessRow | null>(null);
  const [message, setMessage] = useState<{ text: string; bad: boolean } | null>(null);

  const rows = table.data?.rows ?? [];
  const appPids = table.data?.appPids;
  // Apps own a visible window, as in Task Manager; everything else runs in the background.
  const { apps, background } = useMemo(() => {
    const isApp = new Set(appPids);
    return { apps: rows.filter((r) => isApp.has(r.pid)), background: rows.filter((r) => !isApp.has(r.pid)) };
  }, [rows, appPids]);
  const selRow = rows.find((r) => key(r) === selected) ?? null;

  const act = async (fn: () => Promise<unknown>, ok: string) => {
    try {
      await fn();
      setMessage({ text: ok, bad: false });
      table.refresh();
    } catch (e) {
      setMessage({ text: errorText(e), bad: true });
    }
  };

  const doEnd = async () => {
    if (!confirm) return;
    const { row, tree } = confirm;
    setConfirm(null);
    if (tree) {
      try {
        const r = await api.endProcessTree(row.pid, row.startTimeMs);
        setMessage({
          text: `Ended ${r.ended.length} process(es)${r.failed.length ? `; ${r.failed.length} could not be ended: ${r.failed.map(([pid, n, e]) => `${n} (${pid}): ${e}`).join("; ")}` : ""}`,
          bad: r.failed.length > 0,
        });
      } catch (e) {
        setMessage({ text: errorText(e), bad: true });
      }
      table.refresh();
    } else await act(() => api.endProcess(row.pid, row.name, row.startTimeMs), `Ended ${row.name} (${row.pid})`);
  };

  const screen = { loading: !table.data, selected, onSelect: setSelected, actions: { end: (row, tree) => setConfirm({ row, tree }), details: setDetails, run: act } satisfies RowActions };
  // PID 0 is the System Idle Process: its "CPU" is idle time, not load.
  const total = rows.reduce((a, r) => a + (r.pid === 0 ? 0 : (r.cpuPct ?? 0)), 0);
  return (
    <>
      <div className="flex items-center gap-2 px-5 pb-3 shrink-0">
        <Button variant={treeMode ? "primary" : "default"} onClick={() => setTreeMode(!treeMode)} aria-pressed={treeMode} title="Show every process under the one that started it">
          <ListTree className="size-3.5" /> Tree
        </Button>
        <span className="ml-auto text-muted text-xs tabular">
          {rows.length} processes · {f.pct(total, 0)} CPU in use
        </span>
        <Button disabled={!selRow} variant="danger" onClick={() => selRow && setConfirm({ row: selRow, tree: false })}>
          End task
        </Button>
      </div>
      {message && (
        <div className={cn("mx-5 mb-2 px-3 py-1.5 rounded-sm text-xs border flex items-center", message.bad ? "text-bad border-bad/30 bg-bad/8" : "text-ok border-ok/30 bg-ok/8")} role="status">
          <span className="selectable">{message.text}</span>
          <button className="ml-auto text-muted hover:text-fg" onClick={() => setMessage(null)} aria-label="Dismiss">
            ×
          </button>
        </div>
      )}
      <div className="px-5 shrink-0">
        <StatusNote status={table.data?.status} />
        <ErrorNote error={table.error} />
      </div>
      <div className="flex-1 min-h-0 flex flex-col gap-3 px-5 pb-5">
        {treeMode ? (
          <ProcessScreen id="tree" title="Process tree" noun="processes" rows={rows} tree className="flex-1 min-h-0" {...screen} />
        ) : (
          <>
            {/* As tall as its rows, up to half the space; the background list takes the rest. */}
            <div className="flex flex-col shrink min-h-[170px] max-h-1/2" style={{ height: 100 + 30 * Math.max(apps.length, 1) }}>
              <ProcessScreen id="apps" title="Apps" noun="apps" rows={apps} className="flex-1 min-h-0" {...screen} />
            </div>
            <ProcessScreen id="background" title="Background processes" noun="background processes" rows={background} className="flex-1 min-h-[170px]" {...screen} />
          </>
        )}
      </div>

      <Modal
        open={!!confirm}
        onOpenChange={(o) => !o && setConfirm(null)}
        title={confirm?.tree ? "End process tree?" : "End task?"}
        footer={
          <>
            <Button onClick={() => setConfirm(null)}>Cancel</Button>
            <Button variant="danger" onClick={doEnd} autoFocus>
              {confirm?.tree ? "End process tree" : "End task"}
            </Button>
          </>
        }
      >
        {confirm && (
          <div className="flex flex-col gap-2">
            <p>
              <span className="font-semibold">{confirm.row.name}</span> (PID {confirm.row.pid})
              {confirm.tree ? " and every process it started will be ended." : " will be ended."}
            </p>
            <p className="text-muted">Unsaved data in {confirm.tree ? "these processes" : "this process"} will be lost. Windows-critical processes are refused automatically.</p>
          </div>
        )}
      </Modal>
      {details && <DetailsModal row={details} onClose={() => setDetails(null)} />}
    </>
  );
}

function MenuItem({ children, icon, onSelect, disabled }: { children: ReactNode; icon?: ReactNode; onSelect: () => void; disabled?: boolean }) {
  return (
    <CM.Item disabled={disabled} onSelect={onSelect} className="flex items-center gap-2 h-8 px-2.5 rounded-[3px] outline-none data-[highlighted]:bg-hover data-[disabled]:opacity-40">
      <span className="text-muted w-3.5">{icon}</span>
      {children}
    </CM.Item>
  );
}

function DetailsModal({ row, onClose }: { row: ProcessRow; onClose: () => void }) {
  const d = useQuery(() => api.processDetails(row.pid, row.name), [row.pid]);
  const p = d.data;
  return (
    <Modal open onOpenChange={(o) => !o && onClose()} title={`${row.name} — properties`} wide>
      <ErrorNote error={d.error} />
      <Fields
        rows={[
          ["PID", row.pid],
          ["Parent PID", row.parentPid],
          ["Started", f.dateTime(row.startTimeMs)],
          ["Path", p?.path ?? row.path ?? "Not accessible"],
          ["Command line", <span className="font-mono text-xs break-all">{p?.commandLine ?? (d.loading ? "…" : "Not accessible")}</span>],
          ["Description", p?.file?.description ?? f.DASH],
          ["Company", p?.file?.company ?? f.DASH],
          ["File version", p?.file?.fileVersion ?? f.DASH],
          ["Priority", PRIORITIES.find((x) => x.value === p?.priority)?.label ?? p?.priority ?? f.DASH],
          ["Critical to Windows", p?.critical == null ? "Unknown" : p.critical ? "Yes" : "No"],
          ["Session", row.sessionId],
          ["Threads / handles", `${row.threads} / ${row.handles}`],
          ["Private working set", f.bytes(row.privateWorkingSetBytes)],
          ["Working set", f.bytes(row.workingSetBytes)],
          ["Committed", f.bytes(row.privateBytes)],
          ["GPU dedicated memory", f.bytes(row.gpuDedicatedBytes)],
        ]}
      />
      {p?.accessNote && <p className="mt-3 text-warn text-xs">{p.accessNote}</p>}
      {p?.protectedByPolicy && <p className="mt-2 text-muted text-xs">This process is protected from ending in Veyl.</p>}
    </Modal>
  );
}

function SimpleTable<T>({ rows, columns, rowKey, label, initialSort }: { rows: T[]; columns: Column<T>[]; rowKey: (r: T) => string; label: string; initialSort: SortState }) {
  const [sort, setSort] = useState<SortState | null>(initialSort);
  const [sel, setSel] = useState<string | null>(null);
  return (
    <div className="crt flex-1 min-h-0 mx-5 mb-5 overflow-hidden">
      <DataTable label={label} rows={sortRows(rows, columns, sort)} columns={columns} rowKey={rowKey} sort={sort} onSort={setSort} selected={sel} onSelect={setSel} />
    </div>
  );
}

function ServicesTab() {
  const q = useQuery(() => api.services(), [], { intervalMs: 10_000 });
  const [search, setSearch] = useState("");
  const rows = (q.data ?? []).filter((s) => !search || `${s.name} ${s.displayName}`.toLowerCase().includes(search.toLowerCase()));
  const cols: Column<ServiceInfo>[] = [
    { id: "display", header: "Name", width: "minmax(220px,2fr)", sortValue: (s) => s.displayName, cell: (s) => s.displayName },
    { id: "name", header: "Service", width: 160, sortValue: (s) => s.name, cell: (s) => <span className="text-muted">{s.name}</span> },
    { id: "state", header: "State", width: 90, sortValue: (s) => s.state, cell: (s) => <span className={cn("capitalize", s.state === "running" ? "text-ok" : "text-muted")}>{s.state}</span> },
    { id: "start", header: "Startup", width: 90, sortValue: (s) => s.startType, cell: (s) => <span className="capitalize">{s.startType ?? f.DASH}</span> },
    { id: "pid", header: "PID", width: 64, align: "right", sortValue: (s) => s.pid, cell: (s) => s.pid ?? f.DASH },
    { id: "account", header: "Account", width: 170, sortValue: (s) => s.account, cell: (s) => <span className="text-muted" title={s.account ?? ""}>{s.account ?? f.DASH}</span> },
    { id: "path", header: "Binary", width: "minmax(220px,2fr)", sortValue: (s) => s.binaryPath, cell: (s) => <span className="text-muted" title={s.binaryPath ?? ""}>{s.binaryPath ?? f.DASH}</span> },
  ];
  return (
    <>
      <div className="flex items-center gap-2 px-5 py-2 shrink-0">
        <SearchInput placeholder="Search services" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Search services" />
        <span className="text-muted text-xs">Read-only view; service configuration is never changed.</span>
      </div>
      <div className="px-5">
        <ErrorNote error={q.error} />
      </div>
      <SimpleTable label="Services" rows={rows} columns={cols} rowKey={(s) => s.name} initialSort={{ id: "display", desc: false }} />
    </>
  );
}

function StartupTab() {
  const q = useQuery(() => api.startupItems(), []);
  const cols: Column<StartupItem>[] = [
    { id: "name", header: "Name", width: "minmax(180px,1.2fr)", sortValue: (s) => s.name, cell: (s) => s.name },
    {
      id: "enabled",
      header: "Status",
      width: 90,
      sortValue: (s) => (s.enabled === false ? 0 : 1),
      cell: (s) => (s.enabled === false ? <Badge>Disabled</Badge> : <Badge tone="ok">Enabled</Badge>),
    },
    { id: "scope", header: "Scope", width: 80, sortValue: (s) => s.scope, cell: (s) => <span className="capitalize">{s.scope}</span> },
    { id: "location", header: "Location", width: 180, sortValue: (s) => s.location, cell: (s) => <span className="text-muted">{s.location}</span> },
    { id: "command", header: "Command", width: "minmax(260px,3fr)", sortValue: (s) => s.command, cell: (s) => <span className="text-muted font-mono text-xs" title={s.command}>{s.command}</span> },
  ];
  return (
    <>
      <p className="px-5 py-2 text-muted text-xs shrink-0">Registry Run keys and Startup folders. Status reflects Task Manager's enabled/disabled setting. Read-only.</p>
      <div className="px-5">
        <ErrorNote error={q.error} />
      </div>
      <SimpleTable label="Startup applications" rows={q.data ?? []} columns={cols} rowKey={(s) => `${s.location}:${s.name}`} initialSort={{ id: "name", desc: false }} />
    </>
  );
}

function ConnectionsTab() {
  const q = useQuery(() => api.connections(), [], { intervalMs: 3000 });
  const [search, setSearch] = useState("");
  const rows = (q.data ?? []).filter((c) => !search || `${c.processName} ${c.localAddress}:${c.localPort} ${c.remoteAddress}:${c.remotePort} ${c.state}`.toLowerCase().includes(search.toLowerCase()));
  const cols: Column<ConnectionRow>[] = [
    { id: "proc", header: "Process", width: "minmax(160px,1.2fr)", sortValue: (c) => c.processName, cell: (c) => `${c.processName ?? "?"} (${c.pid})` },
    { id: "proto", header: "Protocol", width: 76, sortValue: (c) => c.protocol, cell: (c) => c.protocol },
    { id: "local", header: "Local address", width: "minmax(180px,1fr)", sortValue: (c) => c.localPort, cell: (c) => `${c.localAddress}:${c.localPort}` },
    { id: "remote", header: "Remote address", width: "minmax(180px,1fr)", sortValue: (c) => c.remoteAddress, cell: (c) => (c.remoteAddress ? `${c.remoteAddress}:${c.remotePort}` : f.DASH) },
    { id: "state", header: "State", width: 110, sortValue: (c) => c.state, cell: (c) => c.state ?? f.DASH },
  ];
  return (
    <>
      <div className="flex items-center gap-2 px-5 py-2 shrink-0">
        <SearchInput placeholder="Filter connections" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Filter connections" />
        <span className="text-muted text-xs">Per-connection traffic is not exposed by Windows without administrator rights and is not shown.</span>
      </div>
      <SimpleTable label="Network connections" rows={rows} columns={cols} rowKey={(c) => `${c.protocol}${c.localAddress}${c.localPort}${c.remoteAddress}${c.remotePort}${c.pid}`} initialSort={{ id: "proc", desc: false }} />
    </>
  );
}

export default function Processes() {
  const { params } = useNav();
  const [tab, setTab] = usePref<Tab>("procTab", "processes");
  // A jump from elsewhere names the tab. `params` is a new object per jump, so jumping to the same tab twice works.
  useEffect(() => {
    if (params.tab) setTab(params.tab as Tab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params]);
  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <PageHeader title="Processes" subtitle="Live process list from the kernel's process snapshot; refreshed every 1.5 s" />
      <Tabs
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "processes", label: "Processes" },
          { value: "services", label: "Services" },
          { value: "startup", label: "Startup apps" },
          { value: "connections", label: "Network connections" },
        ]}
      />
      {tab === "processes" && <ProcessesTab />}
      {tab === "services" && <ServicesTab />}
      {tab === "startup" && <StartupTab />}
      {tab === "connections" && <ConnectionsTab />}
    </div>
  );
}
