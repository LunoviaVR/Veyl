import { describe, expect, it } from "vitest";
import { annunciators } from "./annunciators";
import type { AlertSignal, TelemetryFrame } from "./types";

const GB = 1024 ** 3;

function frame(over: { gpuTemp?: number; memory?: number; freeGb?: number } = {}): TelemetryFrame {
  return {
    gpus: { adapters: [{ temperatureC: over.gpuTemp ?? 60 }] },
    memory: { loadPct: over.memory ?? 40 },
    disks: { volumes: [{ mount: "C:\\", totalBytes: 100 * GB, freeBytes: (over.freeGb ?? 50) * GB }] },
  } as unknown as TelemetryFrame;
}

const alert = (metric: string, severity: AlertSignal["severity"]) => ({ metric, severity }) as AlertSignal;
const lit = (tiles: ReturnType<typeof annunciators>) => Object.fromEntries(tiles.filter((t) => t.tone).map((t) => [t.id, t]));

describe("annunciators", () => {
  it("stays dark on a healthy PC, and before the first sample", () => {
    expect(lit(annunciators(frame(), [], {}))).toEqual({});
    expect(lit(annunciators(null, [], { crashes7: 0 }))).toEqual({});
  });

  it("lights the subsystem an active alert belongs to", () => {
    const tiles = lit(annunciators(frame(), [alert("gpu.0.hotspot_c", "warning"), alert("cpu.package_temp_c", "critical")], {}));
    expect(tiles.gpu).toMatchObject({ tone: "warn", note: "Alert", blink: false });
    expect(tiles.cpu).toMatchObject({ tone: "bad", blink: true });
    expect(Object.keys(tiles)).toHaveLength(2);
  });

  it("lights on the built-in limits without an alert, and names the reading", () => {
    const tiles = lit(annunciators(frame({ gpuTemp: 91, memory: 93, freeGb: 5 }), [], {}));
    expect(tiles.gpu).toMatchObject({ tone: "bad", note: "91 °C" });
    expect(tiles.memory).toMatchObject({ tone: "bad", note: "93%" });
    expect(tiles.storage).toMatchObject({ tone: "warn", note: "C: 5% free" });
  });

  it("shows recorded incidents with their count and window, steadily", () => {
    const tiles = lit(annunciators(frame(), [], { crashes7: 2, bsod30: 1 }));
    expect(tiles.crashes).toMatchObject({ tone: "warn", note: "2 · 7 d" });
    expect(tiles.bsod).toMatchObject({ tone: "bad", note: "1 · 30 d" });
    expect(tiles.bsod?.blink).toBeUndefined();
  });
});
