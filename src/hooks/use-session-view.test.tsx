/**
 * SOURCE OF TRUTH KEYWORDS: useSessionView test, initial session read test, event overtakes read test, failed session read test
 * WHAT:  Verifies useSessionView shows the initial `session_get_state` answer, then every SessionStateChanged; that an
 *        event which arrives before the read answers is not overwritten by the older read; and that a failed read
 *        is reported and leaves the view unknown until the next event.
 * WHY:   The pill renders only what this hook returns; showing a stale view after a take started would put the
 *        wrong state on screen.
 * WHERE: Runs in the `web` Vitest project with the bindings' command and lib/echo-events mocked.
 */
import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SessionView } from "@/bindings";

const IDLE: SessionView = {
  status: "idle",
  transcript_id: null,
  elapsed_ms: 0,
  countdown_remaining_ms: null,
  outcome: null,
  error: null,
};

const RECORDING: SessionView = { ...IDLE, status: "recording", elapsed_ms: 1200 };

const mocks = vi.hoisted(() => ({
  handler: null as ((payload: SessionView) => void) | null,
  sessionGetState: vi.fn(),
}));

vi.mock("@/bindings", () => ({
  commands: { sessionGetState: mocks.sessionGetState },
}));

vi.mock("@/lib/echo-events", () => ({
  subscribeEchoEvent: (_name: string, handler: (payload: SessionView) => void) => {
    mocks.handler = handler;
    return () => {
      mocks.handler = null;
    };
  },
}));

const { useSessionView } = await import("./use-session-view");

afterEach(() => {
  vi.clearAllMocks();
  mocks.handler = null;
});

describe("useSessionView", () => {
  it("shows the initial read, then every pushed view", async () => {
    mocks.sessionGetState.mockResolvedValue({ status: "ok", data: IDLE });
    const { result } = renderHook(() => useSessionView());
    expect(result.current).toBeNull();
    await waitFor(() => {
      expect(result.current).toEqual(IDLE);
    });
    act(() => {
      mocks.handler?.(RECORDING);
    });
    expect(result.current).toEqual(RECORDING);
    expect(mocks.sessionGetState).toHaveBeenCalledOnce();
  });

  it("keeps an event that overtook the initial read", async () => {
    let answer: (value: unknown) => void = () => undefined;
    mocks.sessionGetState.mockReturnValue(
      new Promise((resolve) => {
        answer = resolve;
      }),
    );
    const { result } = renderHook(() => useSessionView());
    act(() => {
      mocks.handler?.(RECORDING);
    });
    await act(async () => {
      answer({ status: "ok", data: IDLE });
      await Promise.resolve();
    });
    expect(result.current).toEqual(RECORDING);
  });

  it("reports a failed read and waits for the next event", async () => {
    mocks.sessionGetState.mockResolvedValue({ status: "error", error: { code: "Internal" } });
    const onError = vi.fn();
    const { result } = renderHook(() => useSessionView({ onError }));
    await waitFor(() => {
      expect(onError).toHaveBeenCalledWith({ code: "Internal" });
    });
    expect(result.current).toBeNull();
    act(() => {
      mocks.handler?.(RECORDING);
    });
    expect(result.current).toEqual(RECORDING);
  });
});
