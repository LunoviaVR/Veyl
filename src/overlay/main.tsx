// Overlay window: compact live metrics, click-through, transparent background.
import { listen } from "@tauri-apps/api/event";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import * as f from "../lib/format";
import { api } from "../lib/ipc";
import type { GamingLive, OverlaySettings, TelemetryFrame } from "../lib/types";
import "../styles.css";

function Overlay() {
  const [cfg, setCfg] = useState<OverlaySettings | null>(null);
  const [frame, setFrame] = useState<TelemetryFrame | null>(null);
  const [gaming, setGaming] = useState<GamingLive | null>(null);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    api.settings().then((s) => setCfg(s.overlay)).catch(() => undefined);
    const c = listen<{ overlay: OverlaySettings }>("settings-changed", (e) => setCfg(e.payload.overlay));
    // Latest values are buffered; the display refreshes at the configured interval.
    let latestFrame: TelemetryFrame | null = null;
    let latestGaming: GamingLive | null = null;
    const a = listen<TelemetryFrame>("telemetry", (e) => (latestFrame = e.payload));
    const b = listen<GamingLive>("gaming", (e) => (latestGaming = e.payload));
    const id = window.setInterval(() => {
      setFrame(latestFrame);
      setGaming(latestGaming);
      setTick((t) => t + 1);
    }, cfg?.refreshMs ?? 500);
    return () => {
      window.clearInterval(id);
      a.then((u) => u());
      b.then((u) => u());
      c.then((u) => u());
    };
  }, [cfg?.refreshMs]);

  if (!cfg) return null;
  const g = frame?.gpus.adapters[0];
  const live = gaming?.capturing ? gaming.live : null;
  const items: Record<string, [string, string]> = {
    fps: ["FPS", f.fps(live?.currentFps)],
    frametime: ["Frame", live?.avgFrametimeMs != null ? `${live.avgFrametimeMs.toFixed(1)} ms` : f.DASH],
    low1: ["1% low", f.fps(live?.low1pctFps)],
    gpuUtil: ["GPU", f.pct(g?.utilPct)],
    gpuTemp: ["GPU °C", g?.temperatureC != null ? g.temperatureC.toFixed(0) : f.DASH],
    cpuUtil: ["CPU", f.pct(frame?.cpu.totalPct)],
    cpuTemp: ["CPU °C", frame?.cpu.packageTempC != null ? frame.cpu.packageTempC.toFixed(0) : f.DASH],
    vram: ["VRAM", f.bytes(g?.dedicatedUsedBytes)],
    ram: ["RAM", f.bytes(frame?.memory.usedBytes)],
  };
  const horizontal = cfg.layout === "horizontal";
  return (
    <div
      data-tick={tick}
      className={horizontal ? "flex gap-3 items-center" : "flex flex-col gap-0.5"}
      style={{ fontSize: 13 * cfg.scale, opacity: cfg.opacity, background: "rgba(10,12,16,0.72)", color: "#e8eaee", padding: 8 * cfg.scale, borderRadius: 6, fontFamily: "Segoe UI, sans-serif", width: "fit-content" }}
    >
      {cfg.metrics.map((m) => {
        const it = items[m];
        if (!it) return null;
        return (
          <div key={m} className="flex justify-between gap-3 tabular" style={{ minWidth: horizontal ? undefined : 150 * cfg.scale }}>
            <span style={{ color: "#9aa3b2" }}>{it[0]}</span>
            <span style={{ fontWeight: 600 }}>{it[1]}</span>
          </div>
        );
      })}
    </div>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <Overlay />
    </StrictMode>,
  );
}
