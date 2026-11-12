import { Eraser, Play, Square } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { type Column, DataTable, type SortState, sortRows } from "../components/DataTable";
import { Button, cn, Empty, ErrorNote, PageHeader, Select, TextInput } from "../components/ui";
import * as f from "../../lib/format";
import { usePref, useQuery } from "../../lib/hooks";
import { api, errorText } from "../../lib/ipc";
import type { CaptureState, PacketDetail, PacketField, PacketRow } from "../../lib/types";

const PRESETS: { label: string; filter: string }[] = [
  { label: "DNS", filter: "dns" },
  { label: "HTTP", filter: "http" },
  { label: "TLS", filter: "tls" },
  { label: "SMB", filter: "smb" },
  { label: "ARP", filter: "arp" },
  { label: "TCP resets", filter: "tcp.flags.reset" },
  { label: "DNS errors", filter: "dns.flags.rcode > 0" },
  { label: "HTTP errors", filter: "http.response.code >= 400" },
  { label: "Unreachable", filter: "icmp.type == 3 or icmpv6.type == 1" },
];

/** Rows that report a failure: resets, unreachable, DNS/SMB errors, HTTP 4xx/5xx. */
const PROBLEM = /\bRST\b|unreachable|Error:|No such name|Server failure|Refused|exceeded|^HTTP\/\d\.\d [45]\d\d/i;
const HEX_LIMIT = 8192;

/** Filter text for "show packets with this value". */
function fieldFilter(fl: PacketField): string {
  const v = fl.value.replace(/^(0x[0-9a-f]+|\d+) \(.*\)$/i, "$1");
  return `${fl.key} == ${/^[\w.:/-]+$/.test(v) ? v : `"${v.replace(/"/g, "")}"`}`;
}

function Layers({ d, onPick, onFilter }: { d: PacketDetail; onPick: (r: [number, number]) => void; onFilter: (f: string) => void }) {
  return (
    <div className="text-xs">
      {d.layers.map((l, i) => (
        <details key={`${d.id}-${i}`} open={i > 0} className="border-b border-line/50">
          <summary className="cursor-pointer px-3 py-1.5 font-medium hover:bg-hover" onClick={() => onPick([l.offset, l.len])}>
            {l.name}
            {l.summary && <span className="text-muted font-normal">, {l.summary}</span>}
          </summary>
          {l.fields.map((fl, j) => (
            <div
              key={j}
              className="group flex items-center gap-2 pl-7 pr-3 py-0.5 hover:bg-hover"
              onClick={() => onPick([fl.offset, fl.len])}
              title={fl.key ? `Filter field: ${fl.key}` : undefined}
            >
              <span className="text-muted shrink-0">{fl.label}:</span>
              <span className="selectable truncate tabular">{fl.value}</span>
              {fl.key && (
                <button
                  className="ml-auto shrink-0 text-2xs text-accent opacity-0 group-hover:opacity-100 focus:opacity-100"
                  onClick={(e) => {
                    e.stopPropagation();
                    onFilter(fieldFilter(fl));
                  }}
                >
                  Filter
                </button>
              )}
            </div>
          ))}
        </details>
      ))}
    </div>
  );
}

function Hex({ bytes, hl }: { bytes: number[]; hl: [number, number] | null }) {
  const on = (i: number) => hl != null && hl[1] > 0 && i >= hl[0] && i < hl[0] + hl[1];
  const lines = [];
  for (let o = 0; o < Math.min(bytes.length, HEX_LIMIT); o += 16) {
    const chunk = bytes.slice(o, o + 16);
    lines.push(
      <div key={o} className="flex gap-3 whitespace-pre">
        <span className="text-faint">{o.toString(16).padStart(4, "0")}</span>
        <span className="inline-block w-[49ch]">
          {chunk.map((b, i) => (
            <span key={i} className={cn(on(o + i) && "bg-accent/30 text-fg")}>
              {b.toString(16).padStart(2, "0")}
              {i === 7 ? "  " : " "}
            </span>
          ))}
        </span>
        <span>
          {chunk.map((b, i) => (
            <span key={i} className={cn(on(o + i) && "bg-accent/30 text-fg")}>
              {b >= 32 && b < 127 ? String.fromCharCode(b) : "."}
            </span>
          ))}
        </span>
      </div>,
    );
  }
  return (
    <div className="font-mono text-xs text-muted p-3 selectable">
      {lines}
      {bytes.length > HEX_LIMIT && <p className="mt-1 text-faint">First {HEX_LIMIT.toLocaleString()} of {bytes.length.toLocaleString()} bytes shown.</p>}
    </div>
  );
}

export default function Network() {
  const ifaces = useQuery(() => api.captureInterfaces(), [], { intervalMs: 5000 });
  const [iface, setIface] = useState<string | null>(null); // "" = all interfaces
  const [rows, setRows] = useState<PacketRow[]>([]);
  const [status, setStatus] = useState<{ total: number; evicted: number; phase: CaptureState["state"]; lost: number; error: string | null } | null>(null);
  // Kept across page switches and restarts, like Wireshark's last filter.
  const [applied, setApplied] = usePref("packetFilter", "");
  const [draft, setDraft] = useState(applied);
  const [draftError, setDraftError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [hl, setHl] = useState<[number, number] | null>(null);
  const [sort, setSort] = useState<SortState>({ id: "no", desc: true });
  const after = useRef(0);
  const gen = useRef(0); // bumped on reset so in-flight polls are discarded
  const busy = useRef(false);

  // Default to the busiest connected adapter.
  useEffect(() => {
    if (iface !== null || !ifaces.data) return;
    const best = ifaces.data.filter((i) => i.connected).sort((a, b) => (b.rateBps ?? 0) - (a.rateBps ?? 0))[0];
    setIface(best ? String(best.ifIndex) : "");
  }, [ifaces.data, iface]);

  const reset = () => {
    gen.current++;
    after.current = 0;
    busy.current = false;
    setRows([]);
    setSelected(null);
  };

  const poll = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    const g = gen.current;
    try {
      let more = true;
      while (more && g === gen.current) {
        const p = await api.packets(after.current, applied);
        if (g !== gen.current) return;
        after.current = p.lastId;
        more = p.more;
        setStatus({ total: p.total, evicted: p.evicted, phase: p.state, lost: p.lostEvents + p.lostBuffers, error: p.state === "failed" ? p.error : null });
        // While capturing, the picker shows the interface actually being captured.
        if (p.state === "capturing") setIface(p.interface == null ? "" : String(p.interface));
        setRows((prev) => {
          const kept = prev.length && prev[0]!.id < p.oldestId ? prev.filter((r) => r.id >= p.oldestId) : prev;
          return p.rows.length ? kept.concat(p.rows) : kept;
        });
      }
      setError(null);
    } catch (e) {
      if (g === gen.current) setError(errorText(e));
    } finally {
      if (g === gen.current) busy.current = false;
    }
  }, [applied]);

  useEffect(() => {
    reset();
    void poll();
  }, [poll]);

  const running = status?.phase === "capturing";
  useEffect(() => {
    if (!running) return;
    const t = setInterval(() => void poll(), 700);
    return () => clearInterval(t);
  }, [running, poll]);

  useEffect(() => {
    const t = setTimeout(() => api.checkPacketFilter(draft).then(() => setDraftError(null), (e) => setDraftError(errorText(e))), 200);
    return () => clearTimeout(t);
  }, [draft]);

  const apply = (v: string) => {
    setDraft(v);
    api.checkPacketFilter(v).then(
      () => {
        setDraftError(null);
        setApplied(v.trim());
      },
      (e) => setDraftError(errorText(e)),
    );
  };

  const act = async (fn: () => Promise<unknown>, clearRows: boolean) => {
    setError(null);
    try {
      await fn();
      if (clearRows) reset();
      await poll();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const columns: Column<PacketRow>[] = useMemo(
    () => [
      { id: "no", header: "No.", width: 72, align: "right", sortValue: (r) => r.id, cell: (r) => r.id },
      { id: "time", header: "Time (s)", width: 92, align: "right", sortValue: (r) => r.timeUs, cell: (r) => (r.timeUs / 1e6).toFixed(6) },
      { id: "src", header: "Source", width: "minmax(130px,1fr)", sortValue: (r) => r.src, cell: (r) => r.src },
      { id: "dst", header: "Destination", width: "minmax(130px,1fr)", sortValue: (r) => r.dst, cell: (r) => r.dst },
      { id: "proto", header: "Protocol", width: 76, sortValue: (r) => r.protocol, cell: (r) => r.protocol },
      { id: "len", header: "Length", width: 64, align: "right", sortValue: (r) => r.length, cell: (r) => r.length },
      { id: "info", header: "Info", width: "minmax(260px,3fr)", sortValue: (r) => r.info, cell: (r) => <span title={r.info}>{r.info}</span> },
    ],
    [],
  );
  const view = useMemo(() => (sort.id === "no" ? (sort.desc ? [...rows].reverse() : rows) : sortRows(rows, columns, sort)), [rows, sort, columns]);

  const detail = useQuery(() => (selected ? api.packetDetail(Number(selected)) : Promise.resolve(null)), [selected]);
  useEffect(() => setHl(null), [selected]);

  const ifaceName = (i: number) => ifaces.data?.find((x) => x.ifIndex === i)?.name ?? `Interface ${i}`;
  const d = detail.data;
  const options = [{ value: "", label: "All interfaces" }, ...(ifaces.data ?? []).map((i) => ({ value: String(i.ifIndex), label: `${i.name}${i.connected ? "" : " (disconnected)"}` }))];

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <PageHeader
        title="Packet Capture"
        subtitle="Capture and inspect the packets this PC sends and receives, using Windows' built-in packet capture. Packets stay in memory on this PC and are never saved or sent."
        actions={
          <>
            <Select label="Interface" value={iface ?? ""} onChange={(v) => setIface(v)} options={options} disabled={running} />
            {running ? (
              <Button variant="danger" onClick={() => act(() => api.stopPacketCapture(), false)}>
                <Square className="size-3.5" /> Stop
              </Button>
            ) : (
              <Button variant="primary" disabled={iface === null} onClick={() => act(() => api.startPacketCapture(iface ? Number(iface) : null), true)}>
                <Play className="size-3.5" /> Start capture
              </Button>
            )}
            <Button disabled={!status?.total} onClick={() => act(() => api.clearPackets(), true)}>
              <Eraser className="size-3.5" /> Clear
            </Button>
          </>
        }
      />
      <div className="px-5 pb-3 flex flex-col gap-2 shrink-0">
        <form
          className="flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            apply(draft);
          }}
        >
          <TextInput
            aria-label="Display filter"
            spellCheck={false}
            placeholder="Display filter, e.g. ip.addr == 192.168.1.10 and tcp.port == 443 (Enter to apply)"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            className={cn("flex-1 font-mono", draft.trim() && (draftError ? "border-bad! focus:border-bad!" : "border-ok/60! focus:border-ok!"))}
          />
          <Button type="submit">
            Apply
          </Button>
          {applied && (
            <Button variant="ghost" onClick={() => apply("")}>
              Clear filter
            </Button>
          )}
        </form>
        <div className="flex flex-wrap items-center gap-1.5 text-xs">
          {PRESETS.map((p) => (
            <button key={p.label} onClick={() => apply(p.filter)} aria-pressed={applied === p.filter} className={cn("key label h-7 px-2.5 rounded-[4px] border border-line-strong text-2xs", applied === p.filter ? "bg-hover text-fg" : "bg-raised text-muted hover:text-fg")} title={p.filter}>
              {p.label}
            </button>
          ))}
          <span className="ml-auto text-muted tabular">
            {running ? <span className="text-ok">● Capturing</span> : status?.phase === "failed" ? <span className="text-bad">Failed</span> : status?.phase === "stopped" ? "Stopped" : "Ready"} · {f.num(status?.total ?? 0)} packets
            {applied && ` · ${f.num(rows.length)} match`}
            {!!status?.evicted && ` · ${f.num(status.evicted)} oldest dropped (memory limit)`}
            {!!status?.lost && <span className="text-warn" title="Windows discarded capture data because the capture could not keep up; some packets are missing."> · packets missed by the capture</span>}
          </span>
        </div>
        {draftError && draft.trim() && <p className="text-xs text-bad">{draftError}</p>}
        <ErrorNote error={error ?? status?.error} />
      </div>
      <div className="flex-1 min-h-0 flex flex-col gap-3 mx-5 mb-4">
        <div className="crt flex-[3] min-h-0 overflow-hidden">
          <DataTable
            label="Packets"
            rows={view}
            columns={columns}
            rowKey={(r) => String(r.id)}
            sort={sort}
            onSort={setSort}
            selected={selected}
            onSelect={(k) => setSelected(k)}
            rowClass={(r) => (PROBLEM.test(r.info) ? "text-bad" : undefined)}
            empty={status?.total ? "No packets match the filter" : running ? "Waiting for packets…" : "Start a capture to see packets"}
          />
        </div>
        <div className="flex-[2] min-h-0 grid grid-cols-2 gap-3">
          <div className="crt min-h-0 overflow-auto">
            {d ? (
              <>
                <p className="px-3 py-1.5 text-xs text-faint border-b border-line/60">
                  Packet {d.id} · {new Date(d.timeUtc).toLocaleString(undefined, { year: "numeric", month: "short", day: "numeric", hour: "numeric", minute: "2-digit", second: "2-digit", fractionalSecondDigits: 3 })} · {ifaceName(d.ifIndex)}
                  {d.outbound != null && ` · ${d.outbound ? "sent" : "received"}`}
                </p>
                <Layers d={d} onPick={setHl} onFilter={apply} />
              </>
            ) : (
              <Empty title={detail.error ? "Packet unavailable" : "No packet selected"}>{detail.error ?? "Select a packet to see each protocol layer."}</Empty>
            )}
          </div>
          <div className="crt min-h-0 overflow-auto">{d && <Hex bytes={d.bytes} hl={hl} />}</div>
        </div>
        <p className="text-xs text-faint shrink-0">
          Captures Ethernet and Wi-Fi adapters. Encrypted traffic (HTTPS, QUIC) shows only its headers. Each packet is decoded on its own; streams are not reassembled.
        </p>
      </div>
    </div>
  );
}
