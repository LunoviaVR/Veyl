import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { DASH } from "../lib/format";
import { expandText, formatSource, frameAt, layerColor, layerKind, OVERLAY_DESIGNS, OVERLAY_THEMES, parseOvl, placement, sourceValues } from "./ovl";

// Vitest runs from the repository root.
const file = (design: string, theme: string, f: string) => join(process.cwd(), "public/overlay-themes", design, theme, f);
// latin1 matches Windows-1252 for the bytes these files use (the degree sign).
const load = (design: string, theme: string) => parseOvl(readFileSync(file(design, theme, "layout.ovl"), "latin1"));

describe("bundled RivaTuner layouts", () => {
  it("every design ships every theme", () => {
    for (const d of OVERLAY_DESIGNS) for (const t of OVERLAY_THEMES) for (const f of ["layout.ovl", "atlas.png"]) expect(existsSync(file(d.id, t.id, f)), `${d.id}/${t.id}/${f}`).toBe(true);
  });

  it("parses Sleek Compact: panel bounds, sources and layer kinds", () => {
    const o = load("essential/sleek-compact", "dark");
    expect(o.box).toEqual({ x: 585, y: 24, w: 750, h: 93 });
    expect(o.fontPx).toBe(20);
    expect(o.sources.find((s) => s.name === "Framerate 1% Low")?.key).toBe("AB:00000055");
    expect(o.sources.find((s) => s.name === "GPU clock")?.key).toBe("GPU1 clock");
    expect(layerKind(o.layers[0]!.text)).toEqual({ kind: "image", x: 0, y: 0 });
    const ring = layerKind(o.layers[1]!.text);
    expect(ring).toMatchObject({ kind: "frames", source: "CPU usage", min: 0, max: 102, y: 95, w: 67, h: 67, count: 51, cols: 15 });
    expect(layerKind("<G=Frametime,0,0,1,0,40,10>")).toEqual({ kind: "graph", source: "Frametime", min: 0, max: 40, fill: true });
    expect(o.layers.every((l) => l.text.length > 0)).toBe(true);
    expect(() => parseOvl("<!doctype html><html></html>")).toThrow(); // a missing file served as the app page
  });

  it("parses Cassette Futurism: terminal face, bloom, and every reading it shows exists", () => {
    for (const t of OVERLAY_THEMES) {
      const o = load("cassette/console", t.id);
      expect(o.box).toEqual({ x: 0, y: 0, w: 660, h: 140 });
      expect([o.font, o.fontPx, o.fontWeight]).toEqual(["VT323", 28, 400]);
      expect(o.glow).toBeGreaterThan(0);
      expect(o.glowColor).toBe(t.id === "float" ? "000000" : ""); // no glass behind Float: a dark halo, not bloom
      // A macro naming a source the layout does not declare would be drawn as literal text:
      // once the declared ones are taken out, no %name% pair may be left.
      const known = o.sources.map((s) => s.name).sort((a, b) => b.length - a.length);
      for (const l of o.layers) expect(known.reduce((text, n) => text.split(`%${n}%`).join(""), l.text), `${t.id}: ${l.text}`).not.toMatch(/%[A-Za-z][^%]*%/);
    }
    const bar = layerKind(load("cassette/console", "dark").layers[3]!.text);
    expect(bar).toMatchObject({ kind: "frames", source: "CPU usage", w: 120, h: 16, count: 21, cols: 7 });
    if (bar.kind !== "frames") throw new Error("not frames");
    expect(frameAt(bar, 0)).toEqual({ x: 0, y: 142 }); // nothing lit
    expect(frameAt(bar, 100)).toEqual({ x: 6 * 120, y: 142 + 2 * 16 }); // frame 20: all twenty segments
  });

  it("picks ring frames across the atlas grid", () => {
    const ring = layerKind("<AI=CPU usage,0,0,0,102,0,95,67,67,51,15,0>");
    if (ring.kind !== "frames") throw new Error("not frames");
    expect(frameAt(ring, null)).toEqual({ x: 0, y: 95 });
    expect(frameAt(ring, 100)).toEqual({ x: 5 * 67, y: 95 + 3 * 67 }); // frame 50: row 3, column 5
    expect(frameAt(ring, 250)).toEqual(frameAt(ring, 100));
  });

  it("places the overlay on a 3x3 grid", () => {
    expect(placement("topLeft")).toEqual({ justifyContent: "flex-start", alignItems: "flex-start" });
    expect(placement("topCenter")).toEqual({ justifyContent: "center", alignItems: "flex-start" });
    expect(placement("center")).toEqual({ justifyContent: "center", alignItems: "center" });
    expect(placement("middleRight")).toEqual({ justifyContent: "flex-end", alignItems: "center" });
    expect(placement("bottomCenter")).toEqual({ justifyContent: "center", alignItems: "flex-end" });
  });

  it("formats, expands macros and colours by threshold", () => {
    expect(formatSource({ format: "%.2f", formula: "x / 1000" }, 4850)).toBe("4.85");
    expect(formatSource({ format: "", formula: "" }, 57.6)).toBe("58");
    expect(formatSource({ format: "", formula: "" }, null)).toBe(DASH);
    const o = load("essential/sleek-compact", "dark");
    const byName = sourceValues(o, { "AB:00000055": 84.2, "CPU temperature": 80, "RAM usage": 16_998 });
    expect(expandText("%Framerate 1% Low% · %RAM usage% GB · <FT> ms", byName, 6.94, new Date())).toBe("84 · 16.6 GB · 6.9 ms");
    expect(expandText("<TIME=%I:%M %p>", byName, null, new Date(2026, 0, 1, 21, 5))).toBe("09:05 PM");
    const spec = "FFFFFF,,75,FF9F0A,75,85,FF453A,85,(CPU temperature,1)";
    expect(layerColor(spec, byName)).toBe("#FF9F0A");
    expect(layerColor(spec, sourceValues(o, { "CPU temperature": 90 }))).toBe("#FF453A");
    expect(layerColor(spec, sourceValues(o, {}))).toBe("#FFFFFF");
  });
});
