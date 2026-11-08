// Overlay window: the chosen design drawn with live readings; click-through, transparent background.
import { listen } from "@tauri-apps/api/event";
import { StrictMode, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { api } from "../lib/ipc";
import type { GamingLive, OverlaySettings, TelemetryFrame } from "../lib/types";
import "../styles.css";
import { placement, readings } from "./ovl";
import { OvlView, useOvl } from "./OvlView";

/** Usage history for the CPU / GPU graphs, one sample per refresh. */
const HISTORY = 240;

function Overlay() {
  const [cfg, setCfg] = useState<OverlaySettings | null>(null);
  const [data, setData] = useState<{ frame: TelemetryFrame | null; gaming: GamingLive | null }>({ frame: null, gaming: null });
  const history = useRef<Record<string, number[]>>({ "CPU usage": [], "GPU1 usage": [] });
  const ovl = useOvl(cfg?.design ?? "", cfg?.theme ?? "");

  useEffect(() => {
    api.settings().then((s) => setCfg(s.overlay)).catch(() => undefined);
    const c = listen<{ overlay: OverlaySettings }>("settings-changed", (e) => setCfg(e.payload.overlay));
    // Latest values are buffered; the display refreshes at the configured interval.
    let frame: TelemetryFrame | null = null;
    let gaming: GamingLive | null = null;
    const a = listen<TelemetryFrame>("telemetry", (e) => (frame = e.payload));
    const b = listen<GamingLive>("gaming", (e) => (gaming = e.payload));
    const id = window.setInterval(() => {
      const r = readings(frame, gaming);
      for (const k of ["CPU usage", "GPU1 usage"]) {
        const h = history.current[k]!;
        h.push(r[k] ?? 0);
        if (h.length > HISTORY) h.shift();
      }
      setData({ frame, gaming });
    }, cfg?.refreshMs ?? 500);
    return () => {
      window.clearInterval(id);
      a.then((u) => u());
      b.then((u) => u());
      c.then((u) => u());
    };
  }, [cfg?.refreshMs]);

  if (!cfg || !ovl) return null;
  return (
    // Fills the window and sits where the window is anchored on the game; the rest stays transparent.
    <div style={{ display: "flex", height: "100%", ...placement(cfg.position) }}>
      <div style={{ zoom: cfg.scale, opacity: cfg.opacity }}>
        <OvlView
          ovl={ovl}
          design={cfg.design}
          theme={cfg.theme}
          values={readings(data.frame, data.gaming)}
          graphs={{ ...history.current, Frametime: data.gaming?.capturing ? data.gaming.recentFrametimes : [] }}
          now={new Date()}
        />
      </div>
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
