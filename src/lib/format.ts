// Display formatting. Every formatter renders `null`/`undefined` as an em dash:
// a missing measurement is never shown as zero.

export const DASH = "—";

type Num = number | null | undefined;

const isNum = (v: Num): v is number => typeof v === "number" && Number.isFinite(v);

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/** Binary-prefixed size with 3 significant figures (1 KB = 1024 B, as Windows shows). */
export function bytes(v: Num, digits = 1): string {
  if (!isNum(v)) return DASH;
  let n = Math.abs(v);
  let i = 0;
  while (n >= 1024 && i < UNITS.length - 1) {
    n /= 1024;
    i++;
  }
  const d = i === 0 ? 0 : n >= 100 ? 0 : n >= 10 ? Math.min(digits, 1) : digits;
  return `${v < 0 ? "-" : ""}${n.toFixed(d)} ${UNITS[i]}`;
}

export function rate(v: Num): string {
  return isNum(v) ? `${bytes(v)}/s` : DASH;
}

/** Network rates in bits per second, decimal prefixes (as link speeds are quoted). */
export function bitRate(bytesPerSec: Num): string {
  if (!isNum(bytesPerSec)) return DASH;
  const bits = bytesPerSec * 8;
  const units = ["bps", "Kbps", "Mbps", "Gbps"];
  let n = bits;
  let i = 0;
  while (n >= 1000 && i < units.length - 1) {
    n /= 1000;
    i++;
  }
  return `${n.toFixed(n >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

export function pct(v: Num, digits = 0): string {
  return isNum(v) ? `${v.toFixed(digits)}%` : DASH;
}

export function temp(v: Num): string {
  return isNum(v) ? `${v.toFixed(0)} °C` : DASH;
}

export function mhz(v: Num): string {
  if (!isNum(v)) return DASH;
  return v >= 1000 ? `${(v / 1000).toFixed(2)} GHz` : `${v.toFixed(0)} MHz`;
}

export function num(v: Num, digits = 0): string {
  return isNum(v) ? v.toLocaleString(undefined, { maximumFractionDigits: digits, minimumFractionDigits: digits }) : DASH;
}

export function fps(v: Num): string {
  return isNum(v) ? v.toFixed(v >= 100 ? 0 : 1) : DASH;
}

export function ms(v: Num, digits = 1): string {
  return isNum(v) ? `${v.toFixed(digits)} ms` : DASH;
}

export function duration(secs: Num): string {
  if (!isNum(secs) || secs < 0) return DASH;
  const s = Math.floor(secs);
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  if (d > 0) return `${d}d ${h}h ${m}m`;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${r}s`;
  return `${r}s`;
}

export function dateTime(msUtc: Num): string {
  if (!isNum(msUtc)) return DASH;
  return new Date(msUtc).toLocaleString(undefined, { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function timeOnly(msUtc: Num): string {
  if (!isNum(msUtc)) return DASH;
  return new Date(msUtc).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/** Time of day for today; otherwise the date too, so a time is never mistaken for today's. */
export function whenCompact(msUtc: Num, now = Date.now()): string {
  if (!isNum(msUtc)) return DASH;
  const d = new Date(msUtc);
  if (d.toDateString() === new Date(now).toDateString()) return timeOnly(msUtc);
  return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

export function relative(msUtc: Num, now = Date.now()): string {
  if (!isNum(msUtc)) return DASH;
  const diff = Math.round((now - msUtc) / 1000);
  if (diff < 0) return "just now";
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} h ago`;
  return `${Math.floor(diff / 86400)} d ago`;
}

export function hex(v: Num, width = 8): string {
  return isNum(v) ? `0x${v.toString(16).toUpperCase().padStart(width, "0")}` : DASH;
}

/** Formats a value by its canonical unit symbol (as used by sensors and history). */
export function byUnit(v: Num, unit: string): string {
  switch (unit) {
    case "%":
      return pct(v, 1);
    case "°C":
      return isNum(v) ? `${v.toFixed(1)} °C` : DASH;
    case "B":
      return bytes(v);
    case "B/s":
      return rate(v);
    case "MHz":
      return mhz(v);
    case "ms":
      return ms(v);
    case "RPM":
      return isNum(v) ? `${v.toFixed(0)} RPM` : DASH;
    case "V":
      return isNum(v) ? `${v.toFixed(3)} V` : DASH;
    case "W":
      return isNum(v) ? `${v.toFixed(1)} W` : DASH;
    default:
      return isNum(v) ? `${num(v, Number.isInteger(v) || Math.abs(v) >= 10 ? 0 : 2)}${unit ? ` ${unit}` : ""}` : DASH;
  }
}
