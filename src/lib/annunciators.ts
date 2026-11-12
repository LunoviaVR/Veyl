// The Overview's lamp panel: one tile per subsystem, dark while it is normal.
// A tile lights for an active alert on that subsystem, for the limits the readouts
// already colour by (GPU at 90 °C, memory at 90 %, a volume under 10 % free), or for
// recorded incidents. Nothing here diagnoses anything new.
import type { PageId } from "../App";
import * as f from "./format";
import type { AlertSignal, Severity, TelemetryFrame, Tone } from "./types";

export interface Annunciator {
  id: string;
  label: string;
  /** null = normal, tile stays dark. */
  tone: Tone | null;
  /** Why it is lit: "Alert", a reading, or a count. */
  note?: string;
  /** Flashing is kept for something that is wrong right now. */
  blink?: boolean;
  page: PageId;
  params?: Record<string, string>;
}

export interface IncidentCounts {
  crashes7?: number;
  bsod30?: number;
  unexpected30?: number;
  whea30?: number;
}

export const SEVERITY_TONE: Record<Severity, Tone> = { info: "info", warning: "warn", error: "bad", critical: "bad" };

const worst = (tones: (Tone | null | undefined)[]): Tone | null => (["bad", "warn", "info"] as const).find((t) => tones.includes(t)) ?? null;

export function annunciators(frame: TelemetryFrame | null, alerts: AlertSignal[], counts: IncidentCounts): Annunciator[] {
  /** A live tile: the alerts on these metric prefixes, plus an optional built-in limit. */
  const live = (prefixes: string[], limit?: { tone: Tone; note: string } | null) => {
    const alertTone = worst(alerts.filter((a) => prefixes.some((p) => a.metric.startsWith(p))).map((a) => SEVERITY_TONE[a.severity]));
    const tone = worst([alertTone, limit?.tone]);
    return { tone, note: alertTone ? "Alert" : limit?.note, blink: tone === "bad" };
  };
  const recorded = (n: number | undefined, tone: Tone, window: string) => (n ? { tone, note: `${n} · ${window}` } : { tone: null });

  const hotGpu = frame?.gpus.adapters.find((g) => (g.temperatureC ?? 0) >= 90);
  const memory = frame?.memory.loadPct ?? 0;
  const fullVolume = frame?.disks.volumes.find((v) => v.totalBytes && (v.freeBytes ?? 0) / v.totalBytes < 0.1);

  return [
    { id: "cpu", label: "CPU", ...live(["cpu.", "thermal."]), page: "performance", params: { item: "cpu" } },
    { id: "gpu", label: "GPU", ...live(["gpu."], hotGpu && { tone: "bad", note: f.temp(hotGpu.temperatureC) }), page: "performance", params: { item: "gpu-0" } },
    { id: "memory", label: "Memory", ...live(["memory."], memory >= 90 ? { tone: "bad", note: f.pct(memory) } : null), page: "performance", params: { item: "memory" } },
    {
      id: "storage",
      label: "Storage",
      ...live(["disk.", "volume."], fullVolume && { tone: "warn", note: `${fullVolume.mount.replace("\\", "")} ${f.pct(((fullVolume.freeBytes ?? 0) / fullVolume.totalBytes!) * 100)} free` }),
      page: "hardware",
      params: { tab: "storage" },
    },
    { id: "network", label: "Network", ...live(["net."]), page: "performance", params: { item: "network" } },
    { id: "sensors", label: "Sensors", ...live(["sensor."]), page: "hardware", params: { tab: "sensors" } },
    { id: "crashes", label: "App crash", ...recorded(counts.crashes7, "warn", "7 d"), page: "crashes" },
    { id: "bsod", label: "Blue screen", ...recorded(counts.bsod30, "bad", "30 d"), page: "bsod" },
    { id: "restart", label: "Restart", ...recorded(counts.unexpected30, "warn", "30 d"), page: "bsod" },
    { id: "whea", label: "Hardware", ...recorded(counts.whea30, "bad", "30 d"), page: "stability" },
  ];
}
