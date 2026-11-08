// Live telemetry store. One listener receives backend `telemetry` events and
// keeps a bounded ring of recent frames for real-time charts; components
// subscribe through `useLive` with a selector. Rendering is decoupled from
// collection: React re-renders only on new frames, and charts redraw on rAF.
import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";
import { api } from "./ipc";
import type { GamingLive, TelemetryFrame } from "./types";

/** Frames kept for live charts (5 minutes at 1 Hz). */
export const RING_SIZE = 300;

interface LiveState {
  frame: TelemetryFrame | null;
  ring: TelemetryFrame[];
  gaming: GamingLive | null;
}

let state: LiveState = { frame: null, ring: [], gaming: null };
const subscribers = new Set<() => void>();

function emit() {
  for (const s of subscribers) s();
}

export function pushFrame(f: TelemetryFrame) {
  if (state.frame && f.seq <= state.frame.seq && f.timestampMs <= state.frame.timestampMs) return;
  const ring = state.ring.length >= RING_SIZE ? [...state.ring.slice(state.ring.length - RING_SIZE + 1), f] : [...state.ring, f];
  state = { ...state, frame: f, ring };
  emit();
}

function setGaming(g: GamingLive) {
  state = { ...state, gaming: g };
  emit();
}

function subscribe(cb: () => void) {
  subscribers.add(cb);
  return () => subscribers.delete(cb);
}

export function useLive<T>(select: (s: LiveState) => T): T {
  return useSyncExternalStore(subscribe, () => select(state));
}

export function getLive(): LiveState {
  return state;
}

let started = false;

/** Starts listening once per window. Reports visibility so the backend only emits while someone looks. */
export async function startLive() {
  if (started) return;
  started = true;
  await listen<TelemetryFrame>("telemetry", (e) => pushFrame(e.payload));
  await listen<GamingLive>("gaming", (e) => setGaming(e.payload));
  const report = () => {
    const visible = document.visibilityState === "visible";
    api.setUiVisible(visible).catch(() => undefined);
    if (visible) api.latest().then((f) => f && pushFrame(f)).catch(() => undefined);
  };
  document.addEventListener("visibilitychange", report);
  report();
  api.gamingStatus().then(setGaming).catch(() => undefined);
}

/** Test hook. */
export function resetLive() {
  state = { frame: null, ring: [], gaming: null };
  emit();
}
