/**
 * SOURCE OF TRUTH KEYWORDS: useRunningClock test, live timer test, running clock restart test
 * WHAT:  Verifies useRunningClock counts on from Rust's value while running, restarts for a new key and stops when not
 *        running.
 * WHY:   A live counter must never show a previous take's count.
 * WHERE: Runs in the `web` Vitest project with fake timers.
 */
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRunningClock, type RunningClockOptions } from "./use-running-clock";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useRunningClock", () => {
  it("counts on from the reported value while running and restarts for a new key", () => {
    const { result, rerender } = renderHook((options: RunningClockOptions) => useRunningClock(options), {
      initialProps: { key: "take-1", baseMs: 1_000, running: true },
    });
    expect(result.current).toBe(1_000);
    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(result.current).toBeGreaterThanOrEqual(1_750);
    rerender({ key: "take-2", baseMs: 0, running: true });
    expect(result.current).toBe(0);
    rerender({ key: "take-2", baseMs: 4_000, running: false });
    act(() => {
      vi.advanceTimersByTime(1_000);
    });
    expect(result.current).toBe(4_000);
  });
});
