/**
 * SOURCE OF TRUTH KEYWORDS: subscribeEchoEvent test, unlisten race test, event payload test, subscription failure test
 * WHAT:  Verifies subscribeEchoEvent delivers payloads, stops delivering after unsubscribe, releases a listener that
 *        resolves only after unsubscribe, and reports a failed subscription instead of throwing.
 * WHY:   React effects and the query cache unsubscribe at any moment; a late listener that never unlistens would
 *        leak and keep handling events for an unmounted component.
 * WHERE: Runs in the `web` Vitest project with the generated bindings mocked.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SettingsChanged } from "@/bindings";

type Listener = (event: { payload: SettingsChanged }) => void;

const bindings = vi.hoisted(() => ({
  listener: null as Listener | null,
  listen: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock("@/bindings", () => ({
  events: {
    settingsChanged: { listen: bindings.listen },
    historyChanged: { listen: vi.fn() },
  },
}));

const { ECHO_EVENT_NAMES, subscribeEchoEvent } = await import("./echo-events");

const CHANGE: SettingsChanged = { key: "general.theme", value: { kind: "enum", value: "dark" } };

function listenResolvingWith(unlisten: () => void): void {
  bindings.listen.mockImplementation((listener: Listener) => {
    bindings.listener = listener;
    return Promise.resolve(unlisten);
  });
}

async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

afterEach(() => {
  vi.clearAllMocks();
  bindings.listener = null;
});

describe("subscribeEchoEvent", () => {
  it("delivers payloads until unsubscribed, then releases the listener", async () => {
    listenResolvingWith(bindings.unlisten);
    const handler = vi.fn();
    const unsubscribe = subscribeEchoEvent("settingsChanged", handler);
    await settle();

    bindings.listener?.({ payload: CHANGE });
    expect(handler).toHaveBeenCalledWith(CHANGE);

    unsubscribe();
    expect(bindings.unlisten).toHaveBeenCalledOnce();
    bindings.listener?.({ payload: CHANGE });
    expect(handler).toHaveBeenCalledOnce();
  });

  it("releases a listener that resolves after the unsubscribe", async () => {
    listenResolvingWith(bindings.unlisten);
    const handler = vi.fn();
    const unsubscribe = subscribeEchoEvent("settingsChanged", handler);
    unsubscribe();
    await settle();

    expect(bindings.unlisten).toHaveBeenCalledOnce();
    bindings.listener?.({ payload: CHANGE });
    expect(handler).not.toHaveBeenCalled();
  });

  it("reports a failed subscription and never throws", async () => {
    bindings.listen.mockRejectedValue(new Error("no Tauri here"));
    const onError = vi.fn();
    const unsubscribe = subscribeEchoEvent("settingsChanged", vi.fn(), onError);
    await settle();

    expect(onError).toHaveBeenCalledWith({ code: "Internal" });
    expect(() => {
      unsubscribe();
    }).not.toThrow();
  });

  it("lists every event name from the bindings", () => {
    expect(ECHO_EVENT_NAMES).toEqual(["settingsChanged", "historyChanged"]);
  });
});
