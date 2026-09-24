/**
 * SOURCE OF TRUTH KEYWORDS: useRunningClock, elapsed timer, live timer, m:ss timer, tick, recording timer
 * WHAT:  `useRunningClock({ key, baseMs, running, tickMs })` returns `baseMs` plus the time since this `key` and
 *        `baseMs` arrived, ticking every `tickMs` while `running`; just `baseMs` when not running.
 * WHY:   Rust publishes the take's elapsed time only when its state changes (it never streams a clock), so the UI
 *        counts on from the last value it was given; this is display state, not a copy of domain state, and the next
 *        SessionStateChanged replaces it. The value is keyed, so a new take (or a new base after an undo) never
 *        shows the previous take's count for a tick. Monotonic time (`performance.now`), so a clock change cannot
 *        make it jump.
 * WHERE: The pill's recording timer; any live counter over a value Rust sent.
 */
import { useEffect, useState } from "react";

/** A display clock updates four times a second, enough for an m:ss readout to change on time. */
const DEFAULT_TICK_MS = 250;

export interface RunningClockOptions {
  /** Identifies what is being counted (e.g. the take id); a new key restarts from `baseMs`. */
  readonly key: string;
  /** The elapsed time Rust last reported, in ms. */
  readonly baseMs: number;
  readonly running: boolean;
  readonly tickMs?: number;
}

interface Reading {
  readonly key: string;
  readonly baseMs: number;
  readonly value: number;
}

export function useRunningClock({ key, baseMs, running, tickMs = DEFAULT_TICK_MS }: RunningClockOptions): number {
  const [reading, setReading] = useState<Reading | null>(null);
  useEffect(() => {
    if (!running) {
      return undefined;
    }
    const since = performance.now();
    const timer = setInterval(() => {
      setReading({ key, baseMs, value: baseMs + (performance.now() - since) });
    }, tickMs);
    return () => {
      clearInterval(timer);
    };
  }, [key, baseMs, running, tickMs]);
  if (running && reading?.key === key && reading.baseMs === baseMs) {
    return reading.value;
  }
  return baseMs;
}
