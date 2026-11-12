import { Download, FileSearch, Lock, ShieldAlert } from "lucide-react";
import { useState } from "react";
import { useNav } from "../App";
import { ContextCharts } from "../components/EventViews";
import { Badge, Button, cn, Empty, ErrorNote, Fields, PageHeader, Panel } from "../components/ui";
import * as f from "../../lib/format";
import { useQuery } from "../../lib/hooks";
import { api, errorText } from "../../lib/ipc";
import type { DumpAnalysis, ShutdownIncident } from "../../lib/types";

const RECORD_KINDS: [string, string][] = [
  ["Confirmed bugcheck", "Windows stopped itself with a stop code (a blue screen). Recorded by WER-SystemErrorReporting 1001 and/or a non-zero BugcheckCode in Kernel-Power 41."],
  ["Unexpected shutdown", "Kernel-Power 41 or EventLog 6008 without a stop code: Windows did not shut down cleanly but recorded no blue screen. The logs do not say why."],
  ["Power loss", "One possible cause of an unexpected shutdown (power cut, PSU fault, power button held). Windows cannot record it; it is a hypothesis, never a finding, unless the power button is reported."],
  ["Hardware warning", "WHEA-Logger events: the hardware reported an error. Corrected errors do not stop the system; uncorrected ones usually cause a 0x124 bugcheck."],
  ["Application crash", "One program failed (Application Error 1000). It does not restart Windows — see Crash Reports."],
];

function Explainer() {
  return (
    <details className="crt px-4 py-3 text-sm">
      <summary className="cursor-pointer font-medium">What the different shutdown records mean</summary>
      {/* Each kind is a heading with its text beneath, so the text uses the full width of a narrow column. */}
      <dl className="mt-3 mb-1 flex flex-col gap-3.5">
        {RECORD_KINDS.map(([kind, meaning]) => (
          <div key={kind} className="border-l-2 border-line-strong pl-3">
            <dt className="label text-xs">{kind}</dt>
            <dd className="mt-1 text-muted">{meaning}</dd>
          </div>
        ))}
      </dl>
    </details>
  );
}

function IncidentDetail({ inc }: { inc: ShutdownIncident }) {
  const q = useQuery(() => api.timeline(inc.timeMs, 15 * 60_000), [inc.timeMs]);
  const b = inc.bugcheck;
  return (
    <div className="flex flex-col gap-4">
      <Panel title={inc.kind === "confirmedBugcheck" ? "Confirmed bugcheck" : "Unexpected shutdown (no stop code recorded)"}>
        {b ? (
          <Fields
            rows={[
              ["Stop code", <span className="font-mono">{f.hex(b.code)} {b.name ?? "(not in the built-in table)"}</span>],
              ["Meaning", b.description ?? "See Microsoft's Bug Check Code Reference for this code."],
              ["Usual area", b.area ?? f.DASH],
              ["Parameters", <span className="font-mono text-xs">{b.parameters.map((p) => `0x${p.toString(16)}`).join(", ") || f.DASH}</span>],
              ["Dump file", b.dumpPath ?? "No dump path was recorded"],
              ["Report ID", b.reportId ?? f.DASH],
            ]}
          />
        ) : (
          <p className="text-sm text-muted">
            Windows restarted without a clean shutdown and did not record a stop code.
            {inc.powerButtonPressed ? " The power button was used, which suggests the PC was turned off by hand (for example after it stopped responding)." : " Possible causes include power loss, a forced power-off, a hard hang or a power-supply problem; the logs cannot distinguish them."}
          </p>
        )}
        {inc.previousShutdownText && <p className="mt-2 text-xs text-muted">{inc.previousShutdownText}</p>}
      </Panel>
      <Panel title="Evidence">
        <ul className="text-sm flex flex-col gap-2.5">
          {inc.evidence.map((e) => (
            <li key={`${e.channel}${e.recordId}`}>
              <div className="flex flex-wrap justify-between gap-x-4 text-xs">
                <span className="text-muted tabular">{f.dateTime(e.timeMs)}</span>
                <span className="text-faint">
                  {e.channel} #{e.recordId}
                </span>
              </div>
              <div>{e.title}</div>
            </li>
          ))}
        </ul>
        <p className="text-xs text-faint mt-2">These events are written after Windows starts again, so their times are shortly after the incident.</p>
      </Panel>
      <Panel title="Recorded system state before the restart">{q.data ? <ContextCharts ctx={q.data.context} markerMs={inc.timeMs} /> : <ErrorNote error={q.error} />}</Panel>
    </div>
  );
}

function AnalysisView({ a }: { a: DumpAnalysis }) {
  return (
    <Panel title={`Debugger analysis · ${fileName(a.dumpPath)}`}>
      <Fields
        rows={[
          ["Stop code", a.bugcheckCode != null ? f.hex(a.bugcheckCode) : f.DASH],
          ["Parameters", <span className="font-mono text-xs">{a.bugcheckParameters.map((p) => `0x${p.toString(16)}`).join(", ") || f.DASH}</span>],
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
        <ul className="mt-2 text-xs text-warn list-disc pl-5">
          {a.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
      {a.stack.length > 0 && (
        <details className="mt-2 text-xs">
          <summary className="cursor-pointer text-muted">Stack ({a.stack.length} frames)</summary>
          <pre className="mt-1 font-mono text-2xs overflow-x-auto selectable">{a.stack.join("\n")}</pre>
        </details>
      )}
      <details className="mt-2 text-xs">
        <summary className="cursor-pointer text-muted">Full debugger output</summary>
        <pre className="mt-1 font-mono text-2xs max-h-96 overflow-auto selectable whitespace-pre-wrap">{a.rawOutput}</pre>
      </details>
      <p className="mt-2 text-xs text-faint">The module named by !analyze is where the failure was detected. It is a strong lead, not proof: memory corruption by another driver can surface in a different module.</p>
    </Panel>
  );
}

/** One analysis at a time: the result or error always belongs to the dump it names. */
type Analysis = { kind: "idle" } | { kind: "running"; path: string } | { kind: "done"; result: DumpAnalysis } | { kind: "failed"; path: string; error: string };

const fileName = (path: string) => path.split("\\").pop() ?? path;

export default function Bsod() {
  const { go } = useNav();
  const q = useQuery(() => api.bsodOverview(), [], { events: ["stability-changed"] });
  const [sel, setSel] = useState(0);
  const [run, setRun] = useState<Analysis>({ kind: "idle" });
  const [install, setInstall] = useState<{ kind: "idle" } | { kind: "installing" } | { kind: "failed"; error: string }>({ kind: "idle" });
  const o = q.data;

  const runInstall = () => {
    setInstall({ kind: "installing" });
    api.installDebugger().then(
      () => {
        setInstall({ kind: "idle" });
        q.refresh(); // shows the debugger path and enables Analyze
      },
      (e) => setInstall({ kind: "failed", error: errorText(e) }),
    );
  };
  const inc = o?.incidents[sel];

  const analyze = async (path: string) => {
    setRun({ kind: "running", path });
    try {
      setRun({ kind: "done", result: await api.analyzeDump(path) });
    } catch (e) {
      setRun({ kind: "failed", path, error: errorText(e) });
    }
  };

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <PageHeader title="Blue Screen Analyzer" subtitle="Bugchecks, unexpected shutdowns and kernel crash dumps" />
      <div className="flex-1 min-h-0 flex gap-4 px-5 pb-5">
        <div className="w-[38%] min-w-[360px] max-w-[440px] shrink-0 flex flex-col gap-4 min-h-0 overflow-y-auto pr-1 pb-1">
          <Explainer />
          <ErrorNote error={q.error} />
          <Panel title={`Incidents (${o?.incidents.length ?? 0})`} bodyClass="p-0 overflow-hidden">
            {o?.incidents.length === 0 && <Empty title="No unexpected shutdowns recorded">The System log has no bugcheck, Kernel-Power 41 or unexpected-shutdown events in its retained history.</Empty>}
            {o?.incidents.map((x, i) => (
              // Fixed badge column, so every date and the line under it start at the same place.
              <button
                key={x.timeMs}
                onClick={() => setSel(i)}
                aria-pressed={i === sel}
                className={cn("grid grid-cols-[5.75rem_minmax(0,1fr)] items-center gap-x-3 gap-y-0.5 w-full text-left px-3.5 py-3 border-b border-line/60 last:border-b-0 hover:bg-hover", i === sel && "bg-accent/20 shadow-[inset_3px_0_0_var(--color-accent)]")}
              >
                <Badge tone={x.kind === "confirmedBugcheck" ? "bad" : "warn"} className="justify-center">
                  {x.kind === "confirmedBugcheck" ? "Bugcheck" : "Unexpected"}
                </Badge>
                <span className="text-sm tabular truncate">{f.dateTime(x.timeMs)}</span>
                <span className="col-start-2 text-xs text-muted truncate">{x.bugcheck ? `${f.hex(x.bugcheck.code)} ${x.bugcheck.name ?? ""}` : x.powerButtonPressed ? "No stop code · power button used" : "No stop code recorded"}</span>
              </button>
            ))}
          </Panel>
          <Panel title="Dump files">
            {o && (
              <div className="mb-2 text-xs text-muted">
                {o.debuggerPath ? (
                  <>Debugger: <span className="font-mono text-2xs">{o.debuggerPath}</span></>
                ) : (
                  <>
                    <span className="flex items-start gap-1.5">
                      <ShieldAlert className="size-3.5 mt-px text-warn shrink-0" />
                      <span>
                        Dump analysis needs Debugging Tools for Windows (cdb.exe), which is not installed. Event-log information does not need it.
                      </span>
                    </span>
                    <Button variant="primary" className="mt-2" disabled={install.kind === "installing"} onClick={runInstall}>
                      <Download className="size-3.5" /> {install.kind === "installing" ? "Installing…" : "Install Debugging Tools"}
                    </Button>
                    <p className="mt-1.5 text-xs text-faint">
                      {install.kind === "installing"
                        ? "Downloading from Microsoft. This takes a few minutes."
                        : "Installs only the debugger feature of Microsoft's Windows SDK, with winget, from Microsoft's package source."}{" "}
                      Or install it from the Windows SDK installer, or select cdb.exe in <button className="underline" onClick={() => go("settings", { section: "integrations" })}>Settings</button>.
                    </p>
                    {install.kind === "failed" && <p className="mt-1 text-xs text-bad">{install.error}</p>}
                  </>
                )}
                {!o.elevated && <p className="mt-1 text-xs text-faint">Reading dumps in the Windows folder usually needs administrator rights (Settings → Restart as administrator).</p>}
              </div>
            )}
            {o?.dumps.inaccessible.map((p) => (
              <p key={p} className="text-xs text-warn flex items-center gap-1.5 mb-1">
                <Lock className="size-3.5" /> {p} needs administrator rights to list.
              </p>
            ))}
            {o?.dumps.files.map((d) => (
              <div key={d.path} className="flex items-center gap-2 py-1 text-xs border-b border-line/50">
                <div className="min-w-0">
                  <div className="truncate" title={d.path}>{fileName(d.path)}</div>
                  <div className="text-faint">{d.kind} · {f.bytes(d.sizeBytes)} · {f.dateTime(d.modifiedMs)}</div>
                </div>
                {/* span carries the tooltip: disabled buttons get no pointer events */}
                <span className="ml-auto" title={o.debuggerPath ? undefined : "Needs Debugging Tools for Windows (cdb.exe) — see the note above"}>
                  <Button disabled={run.kind === "running" || !o.debuggerPath} onClick={() => analyze(d.path)}>
                    <FileSearch className="size-3.5" /> {run.kind === "running" && run.path === d.path ? "Analyzing…" : "Analyze"}
                  </Button>
                </span>
              </div>
            ))}
            {o && o.dumps.files.length === 0 && <p className="text-xs text-muted">No dump files were found{o.dumps.inaccessible.length ? " in the accessible locations" : ""}. Windows deletes old minidumps and only keeps them when “Write debugging information” is enabled; this application does not change that setting.</p>}
          </Panel>
        </div>
        <div className="flex-1 min-w-0 overflow-y-auto flex flex-col gap-4 pr-1 pb-1">
          {run.kind === "running" && (
            <Panel title={`Analyzing ${fileName(run.path)}`}>
              <p className="text-sm text-muted">The debugger is reading the dump. This can take a minute, longer the first time symbols are downloaded.</p>
            </Panel>
          )}
          {run.kind === "failed" && <ErrorNote error={`${fileName(run.path)}: ${run.error}`} />}
          {run.kind === "done" && <AnalysisView a={run.result} />}
          {inc ? (
            <IncidentDetail inc={inc} />
          ) : (
            run.kind === "idle" && (
              <div className="crt">
                <Empty title="Nothing selected">Select an incident or analyze a dump.</Empty>
              </div>
            )
          )}
        </div>
      </div>
    </div>
  );
}
