// The designs are drawn in Inter Bold (700) and Extra Bold (800); latin covers the degree sign.
import "@fontsource/inter/latin-700.css";
import "@fontsource/inter/latin-800.css";
import { useEffect, useState } from "react";
import { expandText, frameAt, layerColor, layerKind, type Ovl, parseOvl, sourceValues, themeUrl } from "./ovl";

/** Loads a bundled design; null while loading or when the files are missing. */
export function useOvl(design: string, theme: string): Ovl | null {
  const [ovl, setOvl] = useState<Ovl | null>(null);
  useEffect(() => {
    let alive = true;
    setOvl(null); // never draw the previous layout with the new design's atlas
    if (!design || !theme) return;
    fetch(themeUrl(design, theme, "layout.ovl"))
      .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(`${r.status}`))))
      // The files are ANSI (Windows-1252): the degree sign is one byte.
      .then((b) => alive && setOvl(parseOvl(new TextDecoder("windows-1252").decode(b))))
      .catch(() => alive && setOvl(null));
    return () => {
      alive = false;
    };
  }, [design, theme]);
  return ovl;
}

function Graph({ values, min, max, fill, color, w, h }: { values: number[]; min: number; max: number; fill: boolean; color: string | undefined; w: number; h: number }) {
  const pts = values.slice(-w);
  if (pts.length < 2) return null;
  const y = (v: number) => h - (Math.min(max, Math.max(min, v)) - min) / (max - min || 1) * h;
  const line = pts.map((v, i) => `${((i / (pts.length - 1)) * w).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  return (
    <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} aria-hidden>
      {fill && <polygon points={`0,${h} ${line} ${w},${h}`} fill={color} opacity="0.25" />}
      <polyline points={line} fill="none" stroke={color} strokeWidth="1.5" strokeLinejoin="round" />
    </svg>
  );
}

/** One layout drawn at its own pixel size (1920×1080 pack), cropped to its layers. */
export function OvlView({ ovl, design, theme, values, graphs, now }: { ovl: Ovl; design: string; theme: string; values: Record<string, number | null>; graphs: Record<string, number[]>; now: Date }) {
  const byName = sourceValues(ovl, values);
  const atlas = `url("${themeUrl(design, theme, "atlas.png")}")`;
  const font = `"${ovl.font}", "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif`;
  return (
    <div style={{ position: "relative", width: ovl.box.w, height: ovl.box.h, fontFamily: font, fontWeight: ovl.fontWeight }}>
      {ovl.layers.map((l, i) => {
        const pos = { position: "absolute" as const, left: l.x - ovl.box.x, top: l.y - ovl.box.y };
        const k = layerKind(l.text);
        if (k.kind === "image" || k.kind === "frames") {
          const at = k.kind === "image" ? k : frameAt(k, byName.get(k.source)?.value ?? null);
          return <div key={i} style={{ ...pos, width: l.w, height: l.h, backgroundImage: atlas, backgroundPosition: `${-at.x}px ${-at.y}px` }} />;
        }
        const color = layerColor(l.color, byName);
        if (k.kind === "graph") {
          return (
            <div key={i} style={pos}>
              <Graph values={graphs[k.source] ?? []} min={k.min} max={k.max} fill={k.fill} color={color} w={l.w} h={l.h} />
            </div>
          );
        }
        const justify = l.origin & 2 ? "flex-end" : l.origin & 4 ? "center" : "flex-start";
        return (
          <div
            key={i}
            style={{ ...pos, display: "flex", justifyContent: justify, alignItems: l.h > 0 ? "center" : "flex-start", width: l.w || undefined, height: l.h || undefined, color, fontSize: (ovl.fontPx * l.size) / 100, lineHeight: 1.21, whiteSpace: "nowrap" }}
          >
            {expandText(l.text, byName, values.Frametime ?? null, now)}
          </div>
        );
      })}
    </div>
  );
}
