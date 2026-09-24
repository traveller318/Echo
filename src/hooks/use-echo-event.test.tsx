/**
 * SOURCE OF TRUTH KEYWORDS: useEchoEvent test, latest handler test, unsubscribe on unmount test, enabled option test
 * WHAT:  Verifies useEchoEvent subscribes once, always calls the latest handler without resubscribing, stops on
 *        unmount, and does not subscribe while disabled.
 * WHY:   The pill and progress UIs follow high-rate events through this hook; resubscribing each render would drop
 *        events and a missed unsubscribe would leak a listener per mount.
 * WHERE: Runs in the `web` Vitest project with lib/echo-events mocked.
 */
import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AudioLevel } from "@/bindings";

const events = vi.hoisted(() => ({
  handler: null as ((payload: AudioLevel) => void) | null,
  unsubscribe: vi.fn(),
  subscribe: vi.fn(),
}));

vi.mock("@/lib/echo-events", () => ({
  subscribeEchoEvent: events.subscribe,
}));

const { useEchoEvent } = await import("./use-echo-event");

events.subscribe.mockImplementation((_name: string, handler: (payload: AudioLevel) => void) => {
  events.handler = handler;
  return events.unsubscribe;
});

afterEach(() => {
  vi.clearAllMocks();
  events.handler = null;
});

describe("useEchoEvent", () => {
  it("calls the latest handler without resubscribing and stops on unmount", () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender, unmount } = renderHook(({ handler }) => {
      useEchoEvent("audioLevel", handler);
    }, { initialProps: { handler: first } });

    rerender({ handler: second });
    events.handler?.({ rms: 0.5 });

    expect(events.subscribe).toHaveBeenCalledOnce();
    expect(events.subscribe).toHaveBeenCalledWith("audioLevel", expect.any(Function));
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith({ rms: 0.5 });

    unmount();
    expect(events.unsubscribe).toHaveBeenCalledOnce();
  });

  it("does not subscribe while disabled", () => {
    renderHook(() => {
      useEchoEvent("audioLevel", vi.fn(), { enabled: false });
    });
    expect(events.subscribe).not.toHaveBeenCalled();
  });
});
