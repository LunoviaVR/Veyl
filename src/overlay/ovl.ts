// Renders RivaTuner OverlayEditor layouts (.ovl) with Veyl's own readings. The bundled
// designs come from realHfde/Apple-Style-Inspired-RTSS-Overlay (MIT, see public/overlay-themes),
// except cassette/console, which is Veyl's own and matches the Cassette Futurism theme.
// Supported: image crops <I=…>, value-driven frames <AI=…>, graphs <G=…>, <FT>, <TIME=…>,
// %Source% macros with Format/Formula, text alignment and threshold colours.
import { DASH } from "../lib/format";
import type { GamingLive, GpuSnapshot, TelemetryFrame } from "../lib/types";

export const OVERLAY_THEMES = [
  { id: "dark", label: "Dark" },
  { id: "clear", label: "Smoke" },
  { id: "float", label: "Float" },
  { id: "white", label: "White" },
] as const;

/** Must match `OVERLAY_DESIGNS` in src-tauri/src/settings.rs. */
export const OVERLAY_DESIGNS = [
  { id: "essential/sleek-compact", label: "Sleek Compact" },
  { id: "essential/sleek-rings", label: "Sleek Rings" },
  { id: "essential/sleek-meters", label: "Sleek Meters" },
  { id: "essential/sleek-meters-universal", label: "Sleek Meters Universal" },
  { id: "essential/sleek", label: "Sleek" },
  { id: "essential/simple-sleek", label: "Simple Sleek" },
  { id: "essential/sleek-side", label: "Sleek Side" },
  { id: "essential/performance", label: "Performance" },
  { id: "laptop/card", label: "Laptop · Card" },
  { id: "laptop/island", label: "Laptop · Island" },
  { id: "laptop/island-time", label: "Laptop · Island Time" },
  { id: "laptop/minibar", label: "Laptop · Minibar" },
  { id: "laptop/notch", label: "Laptop · Notch" },
  { id: "tv/box", label: "TV · Box" },
  { id: "tv/line", label: "TV · Line" },
  { id: "tv/pill", label: "TV · Pill" },
  { id: "tv/side", label: "TV · Side" },
  { id: "cassette/console", label: "Cassette Futurism" },
] as const;

/** A 3×3 grid over the game's window, in reading order. Must match `OVERLAY_POSITIONS` in src-tauri/src/settings.rs. */
export const OVERLAY_POSITIONS = [
  { id: "topLeft", label: "Top left" },
  { id: "topCenter", label: "Top center" },
  { id: "topRight", label: "Top right" },
  { id: "middleLeft", label: "Middle left" },
  { id: "center", label: "Center" },
  { id: "middleRight", label: "Middle right" },
  { id: "bottomLeft", label: "Bottom left" },
  { id: "bottomCenter", label: "Bottom center" },
  { id: "bottomRight", label: "Bottom right" },
] as const;

/** Flex alignment that puts content at `position` inside a box (the overlay window or the preview). */
export function placement(position: string): { justifyContent: string; alignItems: string } {
  const edge = (start: boolean, end: boolean) => (start ? "flex-start" : end ? "flex-end" : "center");
  return { justifyContent: edge(position.endsWith("Left"), position.endsWith("Right")), alignItems: edge(position.startsWith("top"), position.startsWith("bottom")) };
}

export const themeUrl = (design: string, theme: string, file: "layout.ovl" | "atlas.png") => `/overlay-themes/${design}/${theme}/${file}`;

export interface OvlSource {
  name: string;
  /** Key into `readings()`: the HAL id, "AB:<SrcId>" for Afterburner, or the provider name. */
  key: string;
  format: string;
  formula: string;
}

export interface OvlLayer {
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
  /** 0 left, 2 right, 4 centre. */
  origin: number;
  size: number;
  color: string;
}

export interface Ovl {
  font: string;
  fontPx: number;
  fontWeight: number;
  /** Bloom around text, in px, in the text's own colour unless `glowColor` (hex) makes it a halo. Veyl's own keys (`Glow`, `GlowColor` under [Master]); RivaTuner layouts have none. */
  glow: number;
  glowColor: string;
  sources: OvlSource[];
  layers: OvlLayer[];
  box: { x: number; y: number; w: number; h: number };
}

export function parseOvl(text: string): Ovl {
  const sections = new Map<string, Map<string, string>>();
  let cur: Map<string, string> | null = null;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    const head = /^\[(.+)\]$/.exec(line);
    if (head) sections.set(head[1]!, (cur = new Map()));
    else if (cur && line.includes("=")) {
      const i = line.indexOf("=");
      cur.set(line.slice(0, i), line.slice(i + 1));
    }
  }
  const get = (s: string, k: string) => sections.get(s)?.get(k) ?? "";
  const int = (s: string, k: string) => Number.parseInt(get(s, k), 10) || 0;

  const sources: OvlSource[] = [];
  for (let i = 0; sections.has(`Source${i}`); i++) {
    const s = `Source${i}`;
    const provider = get(s, "Provider");
    sources.push({
      name: get(s, "Name"),
      key: provider === "HAL" ? get(s, "ID") : provider === "MSI Afterburner" ? `AB:${get(s, "SrcId")}` : provider,
      format: get(s, "Format"),
      formula: get(s, "Formula"),
    });
  }
  const layers: OvlLayer[] = [];
  for (let i = 0; sections.has(`Layer${i}`); i++) {
    const s = `Layer${i}`;
    layers.push({ text: get(s, "Text"), x: int(s, "PositionX"), y: int(s, "PositionY"), w: int(s, "ExtentX"), h: int(s, "ExtentY"), origin: int(s, "ExtentOrigin"), size: int(s, "Size") || 100, color: get(s, "TextColor") });
  }
  const sized = layers.filter((l) => l.w > 0 && l.h > 0);
  if (sized.length === 0) throw new Error("Not an overlay layout");
  const x = Math.min(...sized.map((l) => l.x));
  const y = Math.min(...sized.map((l) => l.y));
  return {
    font: get("Master", "FontFace") || "Inter",
    fontPx: Math.abs(int("Master", "FontHeight")) || 20,
    fontWeight: int("Master", "FontWeight") || 700,
    glow: int("Master", "Glow"),
    glowColor: get("Master", "GlowColor"),
    sources,
    layers,
    box: { x, y, w: Math.max(...sized.map((l) => l.x + l.w)) - x, h: Math.max(...sized.map((l) => l.y + l.h)) - y },
  };
}

type Num = number | null | undefined;
const num = (v: Num): v is number => typeof v === "number" && Number.isFinite(v);
const mb = (bytes: Num) => (num(bytes) ? bytes / 1048576 : null);

/** The adapter the game renders on: the busiest one (an integrated GPU may be listed first). */
export function busiestGpu(frame: TelemetryFrame | null | undefined): GpuSnapshot | undefined {
  return frame?.gpus.adapters.reduce<GpuSnapshot | undefined>((a, b) => (a && (a.utilPct ?? -1) >= (b.utilPct ?? -1) ? a : b), undefined);
}

/** Veyl's readings under the source ids the layouts use, in RivaTuner's units (MHz, MB, °C, W). */
export function readings(frame: TelemetryFrame | null | undefined, gaming: GamingLive | null | undefined): Record<string, number | null> {
  const g = busiestGpu(frame);
  const cpu = frame?.cpu;
  const live = gaming?.capturing ? gaming.live : null;
  const r: Record<string, Num> = {
    "CPU temperature": cpu?.packageTempC,
    "CPU usage": cpu?.totalPct,
    "CPU power": cpu?.packagePowerW,
    "CPU clock": cpu?.effectiveMhz,
    "GPU1 temperature": g?.temperatureC,
    "GPU1 usage": g?.utilPct,
    "GPU1 power": g?.powerW,
    "GPU1 clock": g?.coreClockMhz,
    "GPU1 memory usage": mb(g?.dedicatedUsedBytes),
    "GPU1 fan tachometer": g?.fanRpm,
    "RAM usage": mb(frame?.memory.usedBytes),
    "AB:00000050": live?.currentFps,
    "AB:00000055": live?.low1pctFps,
    "AB:00000056": live?.low01pctFps,
    Frametime: live?.avgFrametimeMs,
  };
  return Object.fromEntries(Object.entries(r).map(([k, v]) => [k, num(v) ? v : null]));
}

/** Applies a source's Formula ("x / 1024") and Format ("%.1f"); integers by default. */
export function formatSource(src: Pick<OvlSource, "format" | "formula">, v: number | null): string {
  if (v == null) return DASH;
  const m = /^x\s*([*/+-])\s*([\d.]+)$/.exec(src.formula.trim());
  if (m) {
    const n = Number(m[2]);
    v = m[1] === "/" ? v / n : m[1] === "*" ? v * n : m[1] === "+" ? v + n : v - n;
  }
  const digits = /%\.(\d)f/.exec(src.format)?.[1];
  return v.toFixed(digits ? Number(digits) : 0);
}

function strftime(fmt: string, d: Date): string {
  const h = d.getHours();
  const pad = (n: number) => String(n).padStart(2, "0");
  const map: Record<string, string> = { H: pad(h), I: pad(h % 12 || 12), M: pad(d.getMinutes()), S: pad(d.getSeconds()), p: h < 12 ? "AM" : "PM", "%": "%" };
  return fmt.replace(/%([HIMSp%])/g, (_, c: string) => map[c]!);
}

/** Display values by source name, after each source's formula, for macros and colour thresholds. */
export function sourceValues(ovl: Ovl, values: Record<string, number | null>): Map<string, { text: string; value: number | null }> {
  return new Map(
    ovl.sources.map((s) => {
      const text = formatSource(s, values[s.key] ?? null);
      return [s.name, { text, value: text === DASH ? null : Number(text) }];
    }),
  );
}

export function expandText(text: string, byName: Map<string, { text: string }>, frametime: number | null, now: Date): string {
  // Names can contain '%' ("Framerate 1% Low"), so match known names only, longest first.
  const names = [...byName.keys()].sort((a, b) => b.length - a.length).map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  let out = text.replace(/<TIME=([^>]*)>/g, (_, f: string) => strftime(f, now)).replace(/<FT>/g, frametime == null ? DASH : frametime.toFixed(1));
  if (names.length) out = out.replace(new RegExp(`%(${names.join("|")})%`, "g"), (_, n: string) => byName.get(n)!.text);
  return out;
}

/** "FFFFFF" or "FFFFFF,,75,FF9F0A,75,85,FF453A,85,(CPU temperature,1)": colour ranges [min, max) of a source. */
export function layerColor(spec: string, byName: Map<string, { value: number | null }>): string | undefined {
  if (!spec) return undefined;
  const [list = "", src = ""] = spec.split("(");
  const parts = list.split(",");
  const v = byName.get(src.split(",")[0]!.trim())?.value;
  if (parts.length < 4 || v == null) return `#${parts[0]}`;
  for (let i = 0; i + 2 < parts.length; i += 3) {
    const [c, lo, hi] = [parts[i]!, parts[i + 1]!, parts[i + 2]!];
    if ((lo === "" || v >= Number(lo)) && (hi === "" || v < Number(hi))) return `#${c}`;
  }
  return `#${parts[parts.length - 3] ?? parts[0]}`;
}

export type LayerKind =
  | { kind: "image"; x: number; y: number }
  | { kind: "frames"; source: string; min: number; max: number; x: number; y: number; w: number; h: number; count: number; cols: number }
  | { kind: "graph"; source: string; min: number; max: number; fill: boolean }
  | { kind: "text" };

export function layerKind(text: string): LayerKind {
  const tag = /^<(I|AI|G)=([^>]*)>$/.exec(text.trim());
  if (!tag) return { kind: "text" };
  const args = tag[2]!.split(",");
  const n = args.map(Number);
  if (tag[1] === "I") return { kind: "image", x: n[2]!, y: n[3]! };
  if (tag[1] === "AI") return { kind: "frames", source: args[0]!, min: n[3]!, max: n[4]!, x: n[5]!, y: n[6]!, w: n[7]!, h: n[8]!, count: n[9]!, cols: n[10]! };
  return { kind: "graph", source: args[0]!, min: n[4]!, max: n[5]!, fill: (n[6]! & 8) !== 0 };
}

/** Atlas cell for a value: frames span [min, max) evenly; frame 0 is empty. */
export function frameAt(k: Extract<LayerKind, { kind: "frames" }>, v: number | null): { x: number; y: number } {
  const i = v == null ? 0 : Math.max(0, Math.min(k.count - 1, Math.floor(((v - k.min) * k.count) / (k.max - k.min))));
  return { x: k.x + (i % k.cols) * k.w, y: k.y + Math.floor(i / k.cols) * k.h };
}
