import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useQuery } from "./hooks";

describe("useQuery", () => {
  it("shows the answer for the latest inputs when they change mid-request", async () => {
    const pending: Record<string, (v: string) => void> = {};
    const load = (k: string) => new Promise<string>((resolve) => (pending[k] = resolve));
    const { result, rerender } = renderHook(({ k }) => useQuery(() => load(k), [k]), { initialProps: { k: "a" } });

    rerender({ k: "b" }); // user picks another item before "a" has loaded
    expect(pending.b).toBeDefined();
    await act(async () => pending.b!("B"));
    await act(async () => pending.a!("A")); // the late answer for "a" must not win
    expect(result.current.data).toBe("B");
  });
});
