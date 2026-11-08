import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { errorText } from "./ipc";

export interface Query<T> {
  data: T | undefined;
  error: string | null;
  loading: boolean;
  refresh: () => void;
}

/**
 * Runs `fn` on mount, every `intervalMs` while the document is visible (if set),
 * and whenever one of `events` (backend event names) fires. Ticks never overlap.
 */
export function useQuery<T>(fn: () => Promise<T>, deps: unknown[], opts: { intervalMs?: number; events?: string[] } = {}): Query<T> {
  const [data, setData] = useState<T>();
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const inFlight = useRef(false);
  // Bumped when the inputs change; answers for older inputs are dropped.
  const gen = useRef(0);
  const fnRef = useRef(fn);
  fnRef.current = fn;

  const run = useCallback(() => {
    if (inFlight.current) return;
    inFlight.current = true;
    const g = gen.current;
    fnRef
      .current()
      .then((d) => {
        if (g !== gen.current) return;
        setData(d);
        setError(null);
      })
      .catch((e) => g === gen.current && setError(errorText(e)))
      .finally(() => {
        if (g !== gen.current) return;
        inFlight.current = false;
        setLoading(false);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  useEffect(() => {
    gen.current++;
    inFlight.current = false; // a request for the old inputs must not block the new one
    setLoading(true);
    run();
    const timer = opts.intervalMs
      ? window.setInterval(() => {
          if (document.visibilityState === "visible") run();
        }, opts.intervalMs)
      : undefined;
    const unlisten: Promise<() => void>[] = (opts.events ?? []).map((name) => listen(name, () => run()));
    return () => {
      if (timer) window.clearInterval(timer);
      unlisten.forEach((p) => p.then((u) => u()).catch(() => undefined));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [run, opts.intervalMs]);

  return { data, error, loading, refresh: run };
}

/** A value persisted in localStorage (per-viewer UI preferences only). */
export function usePref<T>(key: string, initial: T): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => {
    try {
      const s = localStorage.getItem(`pref:${key}`);
      return s ? (JSON.parse(s) as T) : initial;
    } catch {
      return initial;
    }
  });
  const set = useCallback(
    (nv: T) => {
      setV(nv);
      try {
        localStorage.setItem(`pref:${key}`, JSON.stringify(nv));
      } catch {
        // storage unavailable: preference lasts for this session only
      }
    },
    [key],
  );
  return [v, set];
}
