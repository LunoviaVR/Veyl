import { describe, expect, it } from "vitest";
import { bitRate, byUnit, bytes, DASH, duration, hex, mhz, pct, rate, relative, temp, timeOnly, whenCompact } from "./format";

describe("format", () => {
  it("never renders missing values as zero", () => {
    for (const f of [bytes, rate, bitRate, pct, temp, mhz, duration, hex]) {
      expect(f(null)).toBe(DASH);
      expect(f(undefined)).toBe(DASH);
      expect(f(Number.NaN)).toBe(DASH);
    }
    expect(byUnit(null, "°C")).toBe(DASH);
  });

  it("formats bytes with binary prefixes", () => {
    expect(bytes(0)).toBe("0 B");
    expect(bytes(1023)).toBe("1023 B");
    expect(bytes(1536)).toBe("1.5 KB");
    expect(bytes(16 * 1024 ** 3)).toBe("16.0 GB");
    expect(bytes(512 * 1024 ** 2)).toBe("512 MB");
  });

  it("formats rates and bits", () => {
    expect(rate(2048)).toBe("2.0 KB/s");
    expect(bitRate(125_000)).toBe("1.0 Mbps");
    expect(bitRate(10)).toBe("80 bps");
  });

  it("formats clocks, temperatures and durations", () => {
    expect(mhz(4802.6)).toBe("4.80 GHz");
    expect(mhz(850)).toBe("850 MHz");
    expect(temp(43.4)).toBe("43 °C");
    expect(duration(3725)).toBe("1h 2m");
    expect(duration(59)).toBe("59s");
    expect(duration(90061)).toBe("1d 1h 1m");
  });

  it("formats codes and relative times", () => {
    expect(hex(0x13a)).toBe("0x0000013A");
    expect(relative(1000, 31_000)).toBe("30s ago");
    expect(relative(0, 7200_000)).toBe("2 h ago");
  });

  it("adds the date to times that are not today", () => {
    const now = new Date(2026, 10, 7, 22, 0).getTime();
    expect(whenCompact(new Date(2026, 10, 7, 9, 5, 3).getTime(), now)).toBe(timeOnly(new Date(2026, 10, 7, 9, 5, 3).getTime()));
    const earlier = whenCompact(new Date(2026, 10, 1, 21, 21).getTime(), now);
    expect(earlier).toMatch(/Nov/);
    expect(earlier).toMatch(/\b1\b/);
    expect(whenCompact(null)).toBe(DASH);
  });

  it("formats by canonical unit", () => {
    expect(byUnit(65.25, "°C")).toBe("65.3 °C");
    expect(byUnit(1.2346, "V")).toBe("1.235 V");
    expect(byUnit(50, "%")).toBe("50.0%");
    expect(byUnit(3, "")).toBe("3");
    expect(byUnit(3.5, "")).toBe("3.50");
  });
});
