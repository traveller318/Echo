/**
 * SOURCE OF TRUTH KEYWORDS: useDelayedFlag test, useRunningClock test, delayed loading test, live timer test
 * WHAT:  Verifies useDelayedFlag turns true only after its key held for the delay (and restarts for a new key), and
 *        useRunningClock counts on from Rust's value while running, restarts for a new key and stops when not running.
 * WHY:   The pill's processing shimmer must not flicker on fast takes, and its timer must never show a previous
 *        take's count.
 * WHERE: Runs in the `web` Vitest project with fake timers.
 */
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDelayedFlag } from "./use-delayed-flag";
import { useRunningClock, type RunningClockOptions } from "./use-running-clock";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useDelayedFlag", () => {
  it("turns true only after the key held for the delay", () => {
    const { result, rerender } = renderHook(({ key }: { key: string | null }) => useDelayedFlag(key, 150), {
      initialProps: { key: null as string | null },
    });
    expect(result.current).toBe(false);
    rerender({ key: "take-1" });
    act(() => {
      vi.advanceTimersByTime(149);
    });
    expect(result.current).toBe(false);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current).toBe(true);
    rerender({ key: "take-2" });
    expect(result.current).toBe(false);
    rerender({ key: null });
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(result.current).toBe(false);
  });
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
