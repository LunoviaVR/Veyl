import { Download, Eye, FolderOpen, Gamepad2, Play, Square, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { AlignedData } from "uplot";
import { useNav } from "../App";
import { Chart, useThemeKey } from "../components/Chart";
import { type Column, DataTable, type SortState, sortRows } from "../components/DataTable";
import { Badge, Button, cn, Empty, ErrorNote, Fields, Kpi, Modal, PageHeader, Panel, Select, StatusNote, Switch } from "../components/ui";
import * as f from "../lib/format";
import { useQuery } from "../lib/hooks";
import { api, errorText } from "../lib/ipc";
import { useLive } from "../lib/live";
import type { FrameStats, OverlaySettings, SessionRow } from "../lib/types";
import { busiestGpu, OVERLAY_DESIGNS, OVERLAY_POSITIONS, OVERLAY_THEMES, placement, readings } from "../overlay/ovl";
import { OvlView, useOvl } from "../overlay/OvlView";

/** Reduces a long frame-time series for plotting, keeping each bucket's worst frame so spikes stay visible. */
export function downsampleFrames(ft: number[], maxPoints: number): { x: number[]; y: number[] } {
  const x: number[] = [];
  const y: number[] = [];
  let elapsed = 0;
  const step = Math.max(1, Math.ceil(ft.length / maxPoints));
  for (let i = 0; i < ft.length; i += step) {
    let worst = 0;
    let at = elapsed;
    for (let j = i; j < Math.min(i + step, ft.length); j++) {
      const v = ft[j]!;
      if (v > worst) {
        worst = v;
        at = elapsed;
      }
      elapsed += v;
    }
    x.push(at / 1000);
    y.push(worst);
  }
  return { x, y };
}

function StatGrid({ s }: { s: FrameStats }) {
  return (
    <div className="grid grid-cols-3 md:grid-cols-5 gap-x-4 gap-y-3">
      <Kpi label="Current FPS" value={f.fps(s.currentFps)} />
      <Kpi label="Average FPS" value={f.fps(s.avgFps)} />
      <Kpi label="1% low" value={f.fps(s.low1pctFps)} />
      <Kpi label="0.1% low" value={f.fps(s.low01pctFps)} />
      <Kpi label="Stutter" value={f.pct(s.stutterTimePct, 1)} sub={`${s.stutterCount} frames`} tone={(s.stutterTimePct ?? 0) > 5 ? "warn" : undefined} />
      <Kpi label="Avg frame time" value={f.ms(s.avgFrametimeMs)} />
      <Kpi label="P95 frame time" value={f.ms(s.p95FrametimeMs)} />
      <Kpi label="P99 frame time" value={f.ms(s.p99FrametimeMs)} />
      <Kpi label="Max frame time" value={f.ms(s.maxFrametimeMs)} />
      <Kpi label="Pacing variation" value={f.ms(s.pacingVariationMs, 2)} sub="mean |Δ| between frames" />
    </div>
  );
}

function SessionModal({ id, onClose }: { id: number; onClose: () => void }) {
  const q = useQuery(() => api.gameSession(id), [id]);
  const theme = useThemeKey();
  const plot = useMemo(() => {
    if (!q.data) return null;
    const d = downsampleFrames(q.data.frametimes, 4000);
    return [d.x, d.y] as AlignedData;
  }, [q.data]);
  const s = q.data?.session;
  return (
    <Modal open onOpenChange={(o) => !o && onClose()} title={s ? `${s.processName} — ${f.dateTime(s.startedMs)}` : "Session"} wide>
      <ErrorNote error={q.error} />
      {s && plot && (
        <div className="flex flex-col gap-4">
          <StatGrid s={s.stats} />
          <div>
            <h3 className="text-[12px] text-muted mb-1">Frame times (worst frame per point; {q.data!.frametimes.length.toLocaleString()} frames)</h3>
            <Chart data={plot} series={[{ label: "Frame time", color: 3, unit: "ms" }]} time={false} height={200} xLabel={(v) => f.duration(v)} themeKey={theme} />
          </div>
          <Fields
            rows={[
              ["Duration", f.duration((s.endedMs - s.startedMs) / 1000)],
              ["API / present mode", `${s.stats.runtime ?? f.DASH} · ${s.stats.presentMode ?? f.DASH}`],
              ["Frames ignored from other swap chains", f.num(s.stats.otherSwapChainFrames)],
              ["System CPU (avg / max)", s.context.cpuPct ? `${f.pct(s.context.cpuPct.avg)} / ${f.pct(s.context.cpuPct.max)}` : "Not recorded"],
              ["GPU (avg / max)", s.context.gpuPct ? `${f.pct(s.context.gpuPct.avg)} / ${f.pct(s.context.gpuPct.max)}` : "Not recorded"],
              ["Game process CPU / GPU (avg)", `${s.context.processCpuPct ? f.pct(s.context.processCpuPct.avg) : f.DASH} / ${s.context.processGpuPct ? f.pct(s.context.processGpuPct.avg) : f.DASH}`],
              ["GPU temperature (avg / max)", s.context.gpuTempC ? `${f.temp(s.context.gpuTempC.avg)} / ${f.temp(s.context.gpuTempC.max)}` : "Not recorded"],
              ["CPU temperature (avg / max)", s.context.cpuTempC ? `${f.temp(s.context.cpuTempC.avg)} / ${f.temp(s.context.cpuTempC.max)}` : "Not available"],
              ["VRAM (max)", s.context.vramBytes ? f.bytes(s.context.vramBytes.max) : "Not recorded"],
              ["RAM in use (max)", s.context.ramUsedBytes ? f.bytes(s.context.ramUsedBytes.max) : "Not recorded"],
            ]}
          />
          <Bottleneck s={s} />
        </div>
      )}
    </Modal>
  );
}

/** Evidence-based hint only: states the recorded utilization pattern, never a verdict. */
function Bottleneck({ s }: { s: SessionRow }) {
  const gpu = s.context.gpuPct?.avg;
  const proc = s.context.processCpuPct?.avg;
  if (gpu == null) return null;
  let text: string;
  if (gpu >= 95) text = `The GPU averaged ${f.pct(gpu)} busy: frame rate was most likely limited by the GPU.`;
  else if (gpu < 75) text = `The GPU averaged only ${f.pct(gpu)} busy, so something else limited the frame rate — commonly the CPU (main/render thread), a frame-rate cap or V-Sync. Whole-CPU utilization can look low even when one thread is saturated.`;
  else text = `GPU utilization averaged ${f.pct(gpu)}; no single limit is evident from these measurements.`;
  return (
    <p className="text-[12.5px] text-muted border-l-2 border-line-strong pl-3">
      {text}
      {proc != null && ` The game process used ${f.pct(proc)} of total CPU time on average.`}
    </p>
  );
}

function CompareModal({ a, b, onClose }: { a: SessionRow; b: SessionRow; onClose: () => void }) {
  const rows: [string, (s: SessionRow) => number | null, (v: number | null) => string, boolean][] = [
    ["Average FPS", (s) => s.stats.avgFps, f.fps, true],
    ["1% low FPS", (s) => s.stats.low1pctFps, f.fps, true],
    ["0.1% low FPS", (s) => s.stats.low01pctFps, f.fps, true],
    ["Avg frame time", (s) => s.stats.avgFrametimeMs, (v) => f.ms(v), false],
    ["P99 frame time", (s) => s.stats.p99FrametimeMs, (v) => f.ms(v), false],
    ["Stutter time", (s) => s.stats.stutterTimePct, (v) => f.pct(v, 1), false],
    ["Pacing variation", (s) => s.stats.pacingVariationMs, (v) => f.ms(v, 2), false],
    ["GPU avg", (s) => s.context.gpuPct?.avg ?? null, (v) => f.pct(v), true],
    ["GPU temp max", (s) => s.context.gpuTempC?.max ?? null, f.temp, false],
  ];
  return (
    <Modal open onOpenChange={(o) => !o && onClose()} title="Compare sessions" wide>
      <table className="w-full text-[12.5px] tabular">
        <thead className="text-muted text-[11.5px]">
          <tr>
            <th className="text-left font-normal py-1">Metric</th>
            <th className="text-right font-normal">{a.processName} · {f.dateTime(a.startedMs)}</th>
            <th className="text-right font-normal">{b.processName} · {f.dateTime(b.startedMs)}</th>
            <th className="text-right font-normal">Change</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([label, get, fmt, higherBetter]) => {
            const va = get(a);
            const vb = get(b);
            const d = va != null && vb != null && va !== 0 ? ((vb - va) / va) * 100 : null;
            const good = d == null ? null : higherBetter ? d > 0 : d < 0;
            return (
              <tr key={label} className="border-t border-line/60">
                <td className="py-1">{label}</td>
                <td className="text-right">{fmt(va)}</td>
                <td className="text-right">{fmt(vb)}</td>
                <td className={cn("text-right", good === true && "text-ok", good === false && "text-bad")}>{d == null ? f.DASH : `${d > 0 ? "+" : ""}${d.toFixed(1)}%`}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </Modal>
  );
}

/** Design, theme and position pickers with a live preview drawn exactly like the in-game overlay. */
function OverlayStyle({ overlay, onPick, onPosition }: { overlay: OverlaySettings; onPick: (design: string, theme: string) => void; onPosition: (position: string) => void }) {
  const { design, theme, position } = overlay;
  const ovl = useOvl(design, theme);
  const frame = useLive((s) => s.frame);
  const ring = useLive((s) => s.ring);
  const gaming = useLive((s) => s.gaming);
  const graphs = {
    "CPU usage": ring.map((r) => r.cpu.totalPct ?? 0),
    "GPU1 usage": ring.map((r) => busiestGpu(r)?.utilPct ?? 0),
    Frametime: gaming?.capturing ? gaming.recentFrametimes : [],
  };
  return (
    <Panel
      title="Overlay style"
      actions={
        <>
          <Select label="Overlay design" value={design} onChange={(d) => onPick(d, theme)} options={OVERLAY_DESIGNS.map((d) => ({ value: d.id, label: d.label }))} />
          <div role="radiogroup" aria-label="Overlay theme" className="flex">
            {OVERLAY_THEMES.map((t) => (
              <Button key={t.id} role="radio" aria-checked={t.id === theme} variant={t.id === theme ? "primary" : "default"} className="rounded-none first:rounded-l-md last:rounded-r-md -ml-px" onClick={() => onPick(design, t.id)}>
                {t.label}
              </Button>
            ))}
          </div>
        </>
      }
    >
      <div className="flex gap-3">
        <div className="flex flex-col gap-1.5 shrink-0">
          <span className="text-[12px] text-muted">Position on the game</span>
          <div role="radiogroup" aria-label="Overlay position" className="grid grid-cols-3 gap-1 p-1 rounded-md border border-line-strong bg-raised w-fit">
            {OVERLAY_POSITIONS.map((p) => (
              <button
                key={p.id}
                type="button"
                role="radio"
                aria-checked={p.id === position}
                aria-label={p.label}
                title={p.label}
                className={cn("size-6 rounded-sm transition-colors", p.id === position ? "bg-accent" : "bg-line-strong/60 hover:bg-line-strong")}
                onClick={() => onPosition(p.id)}
              />
            ))}
          </div>
        </div>
        {/* The game window: a sky-and-ground backdrop so see-through themes look as they do in a game, with the overlay at the chosen spot. */}
        <div className="flex-1 min-w-0 h-64 rounded-md overflow-hidden flex p-4" style={{ background: "linear-gradient(180deg, #5d7fb3 0%, #9db3cf 58%, #45603f 58%, #2c3c2b 100%)", ...placement(position) }}>
          {ovl && (
            <div style={{ zoom: Math.min(1, 560 / ovl.box.w, 220 / ovl.box.h) }}>
              <OvlView ovl={ovl} design={design} theme={theme} values={readings(frame, gaming)} graphs={graphs} now={new Date()} />
            </div>
          )}
        </div>
      </div>
      <p className="text-[11.5px] text-faint mt-2">
        Designs from Apple-Style RTSS Overlays by realHfde (MIT), set in Inter; Cassette Futurism is Veyl's own. Values Veyl cannot read, such as VRAM temperature, show a dash.
      </p>
    </Panel>
  );
}

export default function Gaming() {
  const { params, go, settings, refreshSettings } = useNav();
  const live = useLive((s) => s.gaming);
  const theme = useThemeKey();
  const candidates = useQuery(() => api.gameCandidates(), [], { intervalMs: 3000 });
  const sessions = useQuery(() => api.gameSessions(500), [], { events: ["gaming-session-saved"] });
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<number | null>(null);
  const [compare, setCompare] = useState<number[]>([]);
  const [showCompare, setShowCompare] = useState(false);
  const [sort, setSort] = useState<SortState | null>({ id: "date", desc: true });

  useEffect(() => {
    if (params.pid && !live?.capturing) api.startCapture(Number(params.pid)).catch((e) => setError(errorText(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params.pid]);

  const recent = useMemo(() => {
    const ft = live?.recentFrametimes ?? [];
    return [ft.map((_, i) => i), ft] as AlignedData;
  }, [live?.recentFrametimes]);

  const providerOk = live?.providerStatus.state !== "providerUnavailable";
  const cols: Column<SessionRow>[] = [
    {
      id: "sel",
      header: "",
      width: 34,
      cell: (s) => (
        <input
          type="checkbox"
          aria-label="Select for comparison"
          checked={compare.includes(s.id)}
          onClick={(e) => e.stopPropagation()}
          onChange={(e) => setCompare(e.target.checked ? [...compare, s.id].slice(-2) : compare.filter((x) => x !== s.id))}
        />
      ),
    },
    { id: "game", header: "Application", width: "minmax(160px,1.5fr)", sortValue: (s) => s.processName, cell: (s) => s.processName },
    { id: "date", header: "Started", width: 150, sortValue: (s) => s.startedMs, cell: (s) => f.dateTime(s.startedMs) },
    { id: "dur", header: "Duration", width: 80, align: "right", sortValue: (s) => s.endedMs - s.startedMs, cell: (s) => f.duration((s.endedMs - s.startedMs) / 1000) },
    { id: "avg", header: "Avg FPS", width: 70, align: "right", sortValue: (s) => s.stats.avgFps, cell: (s) => f.fps(s.stats.avgFps) },
    { id: "low1", header: "1% low", width: 66, align: "right", sortValue: (s) => s.stats.low1pctFps, cell: (s) => f.fps(s.stats.low1pctFps) },
    { id: "low01", header: "0.1% low", width: 70, align: "right", sortValue: (s) => s.stats.low01pctFps, cell: (s) => f.fps(s.stats.low01pctFps) },
    { id: "p99", header: "P99", width: 76, align: "right", sortValue: (s) => s.stats.p99FrametimeMs, cell: (s) => f.ms(s.stats.p99FrametimeMs) },
    { id: "stut", header: "Stutter", width: 66, align: "right", sortValue: (s) => s.stats.stutterTimePct, cell: (s) => f.pct(s.stats.stutterTimePct, 1) },
    {
      id: "act",
      header: "",
      width: 96,
      cell: (s) => (
        <span className="flex gap-0.5 justify-end">
          <button className="p-1 text-muted hover:text-fg" aria-label="View" onClick={() => setView(s.id)}>
            <Eye className="size-3.5" />
          </button>
          <button className="p-1 text-muted hover:text-fg" aria-label="Export CSV" onClick={() => api.exportSessionCsv(s.id).catch((e) => setError(errorText(e)))}>
            <Download className="size-3.5" />
          </button>
          <button
            className="p-1 text-muted hover:text-bad"
            aria-label="Delete"
            onClick={() => {
              if (confirm(`Delete the ${s.processName} session from ${f.dateTime(s.startedMs)}?`)) api.deleteGameSession(s.id).then(() => setTimeout(sessions.refresh, 300));
            }}
          >
            <Trash2 className="size-3.5" />
          </button>
        </span>
      ),
    },
  ];
  const sessionRows = sessions.data ?? [];
  const cmp = compare.map((id) => sessionRows.find((s) => s.id === id)).filter((s): s is SessionRow => !!s);

  return (
    <div className="flex-1 overflow-y-auto">
      <PageHeader
        title="Gaming"
        subtitle="Frame-time capture from real presentation events via Intel PresentMon"
        actions={
          <>
            <label className="flex items-center gap-2 text-[12.5px]" title="Start the FPS capture by itself when a game is running">
              Auto-start for games
              <Switch
                label="Auto-start capture for games"
                checked={!!settings?.autoCaptureGames}
                onChange={(v) =>
                  api
                    .setAutoCaptureGames(v)
                    .then(refreshSettings)
                    .catch((e) => setError(errorText(e)))
                }
              />
            </label>
            <label className="flex items-center gap-2 text-[12.5px]" title="Shown only while a game is the active window">
              Overlay
              <Switch
                label="Game overlay"
                checked={!!settings?.overlay.enabled}
                onChange={(v) =>
                  api
                    .setOverlayEnabled(v)
                    .then(refreshSettings)
                    .catch((e) => setError(errorText(e)))
                }
              />
            </label>
          </>
        }
      />
      <div className="px-5 pb-5 flex flex-col gap-3">
        <ErrorNote error={error} />
        {!providerOk && (
          <Panel title="Set up frame capture">
            <div className="flex flex-col gap-2 text-[12.5px]">
              <StatusNote status={live?.providerStatus} always />
              <p className="text-muted">
                Veyl measures FPS with Intel PresentMon, which reads the presentation events Windows already records (DirectX 9–12, Vulkan and OpenGL via the graphics kernel). Nothing is injected into games, so anti-cheat systems are not affected. PresentMon is free and MIT-licensed.
              </p>
              <p className="text-muted">
                PresentMon is installed with Veyl in its tools folder. Reinstalling Veyl puts it back, or you can select another PresentMon-x.y.z-x64.exe inside Program Files in Settings.
              </p>
              <div className="flex gap-2">
                <Button onClick={() => api.openAppFolder("tools")}>
                  <FolderOpen className="size-3.5" /> Open tools folder
                </Button>
                <Button onClick={() => go("settings", { section: "integrations" })}>Settings</Button>
              </div>
              <p className="text-muted">Frame capture uses Windows event tracing, which needs administrator rights or membership in the Performance Log Users group.</p>
            </div>
          </Panel>
        )}

        {live?.capturing ? (
          <Panel
            title={
              <span className="flex items-center gap-2 normal-case tracking-normal text-fg text-[12.5px]">
                <span className="size-2 rounded-full bg-bad animate-pulse" /> Capturing {live.processName} (PID {live.pid})
                {live.autoStarted && <Badge tone="info">Started automatically</Badge>}
              </span>
            }
            actions={
              <>
                <span className="text-muted text-[12px] tabular">{f.duration(live.durationSecs)} · {live.frames.toLocaleString()} frames</span>
                <Button variant="danger" onClick={() => api.stopCapture().catch((e) => setError(errorText(e)))}>
                  <Square className="size-3.5" /> Stop &amp; save
                </Button>
              </>
            }
            bodyClass="flex flex-col gap-4"
          >
            <StatusNote status={live.providerStatus} />
            <StatGrid s={live.live} />
            <div>
              <h3 className="text-[12px] text-muted mb-1">Frame times — last {live.recentFrametimes.length} frames · statistics over the last 30 s</h3>
              <Chart data={recent} series={[{ label: "Frame time", color: 3, unit: "ms" }]} time={false} height={180} themeKey={theme} xLabel={() => ""} legend={false} />
            </div>
            <div className="grid grid-cols-3 md:grid-cols-6 gap-3">
              <Kpi label="CPU (system)" value={f.pct(live.context?.cpuPct)} sub={`game ${f.pct(live.context?.processCpuPct)}`} />
              <Kpi label="GPU" value={f.pct(live.context?.gpuPct)} sub={`game ${f.pct(live.context?.processGpuPct)}`} />
              <Kpi label="GPU temp" value={f.temp(live.context?.gpuTempC)} />
              <Kpi label="CPU temp" value={f.temp(live.context?.cpuTempC)} />
              <Kpi label="VRAM" value={f.bytes(live.context?.vramBytes)} />
              <Kpi label="RAM in use" value={f.bytes(live.context?.ramUsedBytes)} />
            </div>
            <p className="text-[11.5px] text-faint">
              {live.runtime ?? "?"} · {live.presentMode ?? "?"}
              {live.otherSwapChainFrames > 0 && ` · ${live.otherSwapChainFrames} frames from other swap chains excluded`}
              {live.lostFrames > 0 && ` · ${live.lostFrames} frames dropped by buffers`}
            </p>
          </Panel>
        ) : (
          <Panel title="Start a capture">
            <p className="text-[12.5px] text-muted mb-2">
              Running games first (from Windows' game list or a Steam, Epic, GOG, EA, Ubisoft or Xbox library), then other applications using the GPU and the foreground window.
              {settings?.autoCaptureGames ? " Games start capturing by themselves; use Capture for anything else." : " Choose what to measure."}
              {" "}A game not marked Game? Press Win+G in it and tick "Remember this is a game".
            </p>
            {live?.providerStatus.state !== "available" && live?.providerStatus.state !== "providerUnavailable" && <StatusNote status={live?.providerStatus} />}
            <div className="flex flex-col divide-y divide-line/60">
              {(candidates.data ?? []).map((c) => (
                <div key={c.pid} className="flex items-center gap-3 py-1.5 text-[12.5px]">
                  <Gamepad2 className="size-4 text-faint" />
                  <span className="font-medium">{c.name}</span>
                  <span className="text-muted tabular">PID {c.pid}</span>
                  {c.isGame && <Badge tone="ok">Game</Badge>}
                  {c.foreground && <Badge tone="info">Foreground</Badge>}
                  <span className="ml-auto text-muted tabular">GPU {f.pct(c.gpuPct)} · {f.bytes(c.gpuDedicatedBytes)}</span>
                  <Button variant="primary" disabled={!providerOk} onClick={() => api.startCapture(c.pid).catch((e) => setError(errorText(e)))}>
                    <Play className="size-3.5" /> Capture
                  </Button>
                </div>
              ))}
              {candidates.data?.length === 0 && <p className="text-muted text-[12.5px] py-2">No application is using the GPU right now.</p>}
            </div>
          </Panel>
        )}

        {settings && (
          <OverlayStyle
            overlay={settings.overlay}
            onPick={(d, t) =>
              api
                .setOverlayStyle(d, t)
                .then(refreshSettings)
                .catch((e) => setError(errorText(e)))
            }
            onPosition={(p) =>
              api
                .setOverlayPosition(p)
                .then(refreshSettings)
                .catch((e) => setError(errorText(e)))
            }
          />
        )}

        <Panel
          title="Recorded sessions"
          actions={
            <Button disabled={cmp.length !== 2} onClick={() => setShowCompare(true)}>
              Compare selected ({cmp.length}/2)
            </Button>
          }
          bodyClass="p-0"
        >
          {sessionRows.length === 0 ? (
            <Empty title="No sessions recorded yet">Sessions are saved when you stop a capture or the game exits.</Empty>
          ) : (
            <div className="h-80">
              <DataTable label="Game sessions" rows={sortRows(sessionRows, cols, sort)} columns={cols} rowKey={(s) => String(s.id)} sort={sort} onSort={setSort} onActivate={(s) => setView(s.id)} />
            </div>
          )}
        </Panel>

        <details className="text-[12px] text-muted border border-line rounded-md bg-panel px-3 py-2">
          <summary className="cursor-pointer text-fg">How these numbers are calculated</summary>
          <ul className="list-disc pl-5 mt-2 flex flex-col gap-1">
            <li>Frame time = time between consecutive presents of the game's main swap chain (PresentMon MsBetweenPresents).</li>
            <li>Average FPS = frames ÷ total time (1000 ÷ mean frame time), not the mean of per-frame FPS.</li>
            <li>1% / 0.1% low = 1000 ÷ mean of the slowest 1% / 0.1% of frame times.</li>
            <li>P95 / P99 = nearest-rank percentile of frame times.</li>
            <li>Stutter = a frame longer than {settings?.stutterFactor ?? 2.5}× the median frame time; stutter % is their share of total time.</li>
            <li>Pacing variation = mean absolute change between consecutive frame times.</li>
          </ul>
        </details>
      </div>
      {view != null && <SessionModal id={view} onClose={() => setView(null)} />}
      {showCompare && cmp.length === 2 && <CompareModal a={cmp[0]!} b={cmp[1]!} onClose={() => setShowCompare(false)} />}
    </div>
  );
}
