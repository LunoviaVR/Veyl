import { FileSearch, Lock, ShieldAlert } from "lucide-react";
import { useState } from "react";
import { useNav } from "../App";
import { ContextCharts } from "../components/EventViews";
import { Badge, Button, cn, Empty, ErrorNote, Fields, PageHeader, Panel } from "../components/ui";
import * as f from "../lib/format";
import { useQuery } from "../lib/hooks";
import { api, errorText } from "../lib/ipc";
import type { DumpAnalysis, ShutdownIncident } from "../lib/types";

function Explainer() {
  return (
    <details className="border border-line rounded-md bg-panel px-3 py-2 text-[12.5px]">
      <summary className="cursor-pointer font-medium">What the different shutdown records mean</summary>
      <dl className="mt-2 grid grid-cols-[170px_1fr] gap-x-4 gap-y-1.5 text-muted">
        <dt className="text-fg">Confirmed bugcheck</dt>
        <dd>Windows stopped itself with a stop code (a blue screen). Recorded by WER-SystemErrorReporting 1001 and/or a non-zero BugcheckCode in Kernel-Power 41.</dd>
        <dt className="text-fg">Unexpected shutdown</dt>
        <dd>Kernel-Power 41 or EventLog 6008 without a stop code: Windows did not shut down cleanly but recorded no blue screen. The logs do not say why.</dd>
        <dt className="text-fg">Power loss</dt>
        <dd>One possible cause of an unexpected shutdown (power cut, PSU fault, power button held). Windows cannot record it; it is a hypothesis, never a finding, unless the power button is reported.</dd>
        <dt className="text-fg">Hardware warning</dt>
        <dd>WHEA-Logger events: the hardware reported an error. Corrected errors do not stop the system; uncorrected ones usually cause a 0x124 bugcheck.</dd>
        <dt className="text-fg">Application crash</dt>
        <dd>One program failed (Application Error 1000). It does not restart Windows — see Crash Reports.</dd>
      </dl>
    </details>
  );
}

function IncidentDetail({ inc }: { inc: ShutdownIncident }) {
  const q = useQuery(() => api.timeline(inc.timeMs, 15 * 60_000), [inc.timeMs]);
  const b = inc.bugcheck;
  return (
    <div className="flex flex-col gap-3">
      <Panel title={inc.kind === "confirmedBugcheck" ? "Confirmed bugcheck" : "Unexpected shutdown (no stop code recorded)"}>
        {b ? (
          <Fields
            rows={[
              ["Stop code", <span className="font-mono">{f.hex(b.code)} {b.name ?? "(not in the built-in table)"}</span>],
              ["Meaning", b.description ?? "See Microsoft's Bug Check Code Reference for this code."],
              ["Usual area", b.area ?? f.DASH],
              ["Parameters", <span className="font-mono text-[11.5px]">{b.parameters.map((p) => `0x${p.toString(16)}`).join(", ") || f.DASH}</span>],
              ["Dump file", b.dumpPath ?? "No dump path was recorded"],
              ["Report ID", b.reportId ?? f.DASH],
            ]}
          />
        ) : (
          <p className="text-[12.5px] text-muted">
            Windows restarted without a clean shutdown and did not record a stop code.
            {inc.powerButtonPressed ? " The power button was used, which suggests the PC was turned off by hand (for example after it stopped responding)." : " Possible causes include power loss, a forced power-off, a hard hang or a power-supply problem; the logs cannot distinguish them."}
          </p>
        )}
        {inc.previousShutdownText && <p className="mt-2 text-[12px] text-muted">{inc.previousShutdownText}</p>}
      </Panel>
      <Panel title="Evidence">
        <ul className="text-[12.5px] flex flex-col gap-1">
          {inc.evidence.map((e) => (
            <li key={`${e.channel}${e.recordId}`} className="flex gap-3">
              <span className="text-muted tabular w-40 shrink-0">{f.dateTime(e.timeMs)}</span>
              <span>{e.title}</span>
              <span className="ml-auto text-faint">{e.channel} #{e.recordId}</span>
            </li>
          ))}
        </ul>
        <p className="text-[11.5px] text-faint mt-2">These events are written after Windows starts again, so their times are shortly after the incident.</p>
      </Panel>
      <Panel title="Recorded system state before the restart">{q.data ? <ContextCharts ctx={q.data.context} markerMs={inc.timeMs} /> : <ErrorNote error={q.error} />}</Panel>
    </div>
  );
}

function AnalysisView({ a }: { a: DumpAnalysis }) {
  return (
    <Panel title={`Debugger analysis · ${a.dumpPath.split("\\").pop()}`}>
      <Fields
        rows={[
          ["Stop code", a.bugcheckCode != null ? f.hex(a.bugcheckCode) : f.DASH],
          ["Parameters", <span className="font-mono text-[11.5px]">{a.bugcheckParameters.map((p) => `0x${p.toString(16)}`).join(", ") || f.DASH}</span>],
          ["Probably caused by (debugger heuristic)", a.imageName ? `${a.imageName}${a.moduleName ? ` (module ${a.moduleName})` : ""}` : f.DASH],
          ["Image version", a.imageVersion ?? f.DASH],
          ["Process", a.processName ?? f.DASH],
          ["Failing symbol", a.symbolName ?? f.DASH],
          ["Failure bucket", a.failureBucket ?? f.DASH],
          ["Symbols", a.symbolsResolved ? "Resolved" : "Not fully resolved — results are less reliable"],
          ["Symbol server", a.symbolServerUsed ? "Microsoft symbol server (enabled in Settings)" : "Local cache only"],
          ["Duration", `${(a.durationMs / 1000).toFixed(1)} s`],
        ]}
      />
      {a.warnings.length > 0 && (
        <ul className="mt-2 text-[12px] text-warn list-disc pl-5">
          {a.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
      {a.stack.length > 0 && (
        <details className="mt-2 text-[12px]">
          <summary className="cursor-pointer text-muted">Stack ({a.stack.length} frames)</summary>
          <pre className="mt-1 font-mono text-[11px] overflow-x-auto selectable">{a.stack.join("\n")}</pre>
        </details>
      )}
      <details className="mt-2 text-[12px]">
        <summary className="cursor-pointer text-muted">Full debugger output</summary>
        <pre className="mt-1 font-mono text-[11px] max-h-96 overflow-auto selectable whitespace-pre-wrap">{a.rawOutput}</pre>
      </details>
      <p className="mt-2 text-[11.5px] text-faint">The module named by !analyze is where the failure was detected. It is a strong lead, not proof: memory corruption by another driver can surface in a different module.</p>
    </Panel>
  );
}

export default function Bsod() {
  const { go } = useNav();
  const q = useQuery(() => api.bsodOverview(), [], { events: ["stability-changed"] });
  const [sel, setSel] = useState(0);
  const [analysis, setAnalysis] = useState<DumpAnalysis | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const o = q.data;
  const inc = o?.incidents[sel];

  const analyze = async (path: string) => {
    setBusy(path);
    setError(null);
    try {
      setAnalysis(await api.analyzeDump(path));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <PageHeader title="Blue Screen Analyzer" subtitle="Bugchecks, unexpected shutdowns and kernel crash dumps" />
      <div className="flex-1 min-h-0 flex gap-3 px-5 pb-4">
        <div className="w-[400px] shrink-0 flex flex-col gap-3 min-h-0 overflow-y-auto">
          <Explainer />
          <ErrorNote error={q.error} />
          <Panel title={`Incidents (${o?.incidents.length ?? 0})`} bodyClass="p-0">
            {o?.incidents.length === 0 && <Empty title="No unexpected shutdowns recorded">The System log has no bugcheck, Kernel-Power 41 or unexpected-shutdown events in its retained history.</Empty>}
            {o?.incidents.map((x, i) => (
              <button key={x.timeMs} onClick={() => setSel(i)} className={cn("w-full text-left px-3 py-2 border-b border-line/60 hover:bg-hover", i === sel && "bg-accent/10")}>
                <div className="flex items-center gap-2">
                  {x.kind === "confirmedBugcheck" ? <Badge tone="bad">Bugcheck</Badge> : <Badge tone="warn">Unexpected</Badge>}
                  <span className="text-[12.5px] tabular">{f.dateTime(x.timeMs)}</span>
                </div>
                <div className="text-[12px] text-muted mt-0.5 truncate">{x.bugcheck ? `${f.hex(x.bugcheck.code)} ${x.bugcheck.name ?? ""}` : x.powerButtonPressed ? "No stop code · power button used" : "No stop code recorded"}</div>
              </button>
            ))}
          </Panel>
          <Panel title="Dump files">
            {o?.dumps.inaccessible.map((p) => (
              <p key={p} className="text-[12px] text-warn flex items-center gap-1.5 mb-1">
                <Lock className="size-3.5" /> {p} needs administrator rights to list.
              </p>
            ))}
            {o?.dumps.files.map((d) => (
              <div key={d.path} className="flex items-center gap-2 py-1 text-[12px] border-b border-line/50">
                <div className="min-w-0">
                  <div className="truncate" title={d.path}>{d.path.split("\\").pop()}</div>
                  <div className="text-faint">{d.kind} · {f.bytes(d.sizeBytes)} · {f.dateTime(d.modifiedMs)}</div>
                </div>
                <Button className="ml-auto" disabled={!!busy || !o.debuggerPath} onClick={() => analyze(d.path)}>
                  <FileSearch className="size-3.5" /> {busy === d.path ? "Analyzing…" : "Analyze"}
                </Button>
              </div>
            ))}
            {o && o.dumps.files.length === 0 && <p className="text-[12px] text-muted">No dump files were found{o.dumps.inaccessible.length ? " in the accessible locations" : ""}. Windows deletes old minidumps and only keeps them when “Write debugging information” is enabled; this application does not change that setting.</p>}
            <div className="mt-2 text-[12px] text-muted">
              {o?.debuggerPath ? (
                <>Debugger: <span className="font-mono text-[11px]">{o.debuggerPath}</span></>
              ) : (
                <span className="flex items-start gap-1.5">
                  <ShieldAlert className="size-3.5 mt-px text-warn shrink-0" />
                  <span>
                    Dump analysis needs Debugging Tools for Windows (cdb.exe), which is not installed. Install “Debugging Tools for Windows” from the Windows SDK installer, or select cdb.exe in <button className="underline" onClick={() => go("settings", { section: "integrations" })}>Settings</button>. Event-log information above does not need it.
                  </span>
                </span>
              )}
            </div>
            {!o?.elevated && <p className="mt-1 text-[11.5px] text-faint">Reading dumps in the Windows folder usually needs administrator rights (Settings → Restart as administrator).</p>}
          </Panel>
        </div>
        <div className="flex-1 min-w-0 overflow-y-auto flex flex-col gap-3">
          <ErrorNote error={error} />
          {analysis && <AnalysisView a={analysis} />}
          {inc ? <IncidentDetail inc={inc} /> : !analysis && <Empty title="Nothing selected">Select an incident or analyze a dump.</Empty>}
        </div>
      </div>
    </div>
  );
}
