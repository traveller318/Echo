/**
 * SOURCE OF TRUTH KEYWORDS: startQueryInvalidation test, event-driven invalidation test, listener reference count test, never polled
 * WHAT:  Verifies the bridge listens only to events that cached queries declare, invalidates exactly the queries
 *        that named the event, shares one listener between queries, and stops listening when the last query
 *        leaves the cache or the bridge stops.
 * WHY:   This is how every command read stays fresh without polling (02 §2.4); a missed invalidation shows stale
 *        data and an extra listener costs a window high-rate events it never needed.
 * WHERE: Runs in the `web` Vitest project with an injected subscribe function (no Tauri).
 */
import { describe, expect, it, vi } from "vitest";
import type { EchoEventName } from "./echo-events";
import { createEchoQueryClient } from "./query-client";
import { startQueryInvalidation } from "./query-invalidation";

function fakeSubscribe() {
  const handlers = new Map<EchoEventName, () => void>();
  const stops = new Map<EchoEventName, ReturnType<typeof vi.fn>>();
  const subscribe = vi.fn((name: EchoEventName, handler: () => void) => {
    handlers.set(name, handler);
    const stop = vi.fn(() => handlers.delete(name));
    stops.set(name, stop);
    return stop;
  });
  const fire = (name: EchoEventName): void => {
    handlers.get(name)?.();
  };
  return { subscribe, fire, handlers, stops };
}

function seed(
  client: ReturnType<typeof createEchoQueryClient>,
  key: string,
  invalidatedBy: readonly EchoEventName[],
): void {
  client.getQueryCache().build(client, { queryKey: [key], meta: { invalidatedBy } });
  client.setQueryData([key], key);
}

describe("startQueryInvalidation", () => {
  it("listens only to declared events and invalidates exactly the queries that declared them", () => {
    const client = createEchoQueryClient();
    const events = fakeSubscribe();
    seed(client, "settings", ["settingsChanged"]);
    const stop = startQueryInvalidation(client, events.subscribe);
    seed(client, "history", ["historyChanged", "transcriptSaved"]);
    seed(client, "registry", []);

    expect([...events.handlers.keys()].sort()).toEqual(["historyChanged", "settingsChanged", "transcriptSaved"]);

    events.fire("settingsChanged");
    expect(client.getQueryState(["settings"])?.isInvalidated).toBe(true);
    expect(client.getQueryState(["history"])?.isInvalidated).toBe(false);
    expect(client.getQueryState(["registry"])?.isInvalidated).toBe(false);

    stop();
  });

  it("shares one listener per event and releases it with the last query", () => {
    const client = createEchoQueryClient();
    const events = fakeSubscribe();
    const stop = startQueryInvalidation(client, events.subscribe);
    seed(client, "a", ["historyChanged"]);
    seed(client, "b", ["historyChanged", "historyChanged"]);
    expect(events.subscribe).toHaveBeenCalledOnce();

    client.removeQueries({ queryKey: ["a"] });
    expect(events.stops.get("historyChanged")).not.toHaveBeenCalled();
    client.removeQueries({ queryKey: ["b"] });
    expect(events.stops.get("historyChanged")).toHaveBeenCalledOnce();
    expect(events.handlers.size).toBe(0);

    stop();
  });

  it("stops every listener and ignores later queries once stopped", () => {
    const client = createEchoQueryClient();
    const events = fakeSubscribe();
    seed(client, "settings", ["settingsChanged"]);
    const stop = startQueryInvalidation(client, events.subscribe);
    stop();

    expect(events.stops.get("settingsChanged")).toHaveBeenCalledOnce();
    seed(client, "history", ["historyChanged"]);
    expect(events.subscribe).toHaveBeenCalledOnce();
  });
});
