import { describe, expect, it } from "vitest";
import { type Column, sortRows } from "./DataTable";

interface R {
  name: string;
  cpu: number | null;
}
const cols: Column<R>[] = [
  { id: "name", header: "Name", width: 100, cell: (r) => r.name, sortValue: (r) => r.name },
  { id: "cpu", header: "CPU", width: 60, cell: (r) => r.cpu, sortValue: (r) => r.cpu },
];
const rows: R[] = [
  { name: "b", cpu: 5 },
  { name: "a10", cpu: null },
  { name: "a9", cpu: 50 },
];

describe("sortRows", () => {
  it("sorts numbers and keeps missing values last in both directions", () => {
    expect(sortRows(rows, cols, { id: "cpu", desc: true }).map((r) => r.name)).toEqual(["a9", "b", "a10"]);
    expect(sortRows(rows, cols, { id: "cpu", desc: false }).map((r) => r.name)).toEqual(["b", "a9", "a10"]);
  });
  it("sorts strings naturally", () => {
    expect(sortRows(rows, cols, { id: "name", desc: false }).map((r) => r.name)).toEqual(["a9", "a10", "b"]);
  });
  it("leaves order unchanged without a sort", () => {
    expect(sortRows(rows, cols, null)).toBe(rows);
  });
  it("handles large inputs", () => {
    const big: R[] = Array.from({ length: 20_000 }, (_, i) => ({ name: `p${i}`, cpu: i % 7 === 0 ? null : i % 100 }));
    const sorted = sortRows(big, cols, { id: "cpu", desc: true });
    expect(sorted.length).toBe(20_000);
    expect(sorted[0]!.cpu).toBe(99);
    expect(sorted[sorted.length - 1]!.cpu).toBeNull();
  });
});
