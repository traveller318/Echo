/**
 * SOURCE OF TRUTH KEYWORDS: startQueryInvalidation, event-driven invalidation, invalidatedBy, query cache subscription, reference-counted listeners, never polled
 * WHAT:  `startQueryInvalidation(client)` keeps one Rust event listener per event that at least one cached query
 *        declares in `meta.invalidatedBy`; when such an event arrives, exactly the queries that declared it are
 *        invalidated (active ones refetch). Returns the function that stops everything.
 * WHY:   "Invalidated by the matching Rust event, never polled" (02 §2.4) without a central table to edit: each query
 *        names its own events and this bridge follows the query cache. Listeners exist only while a query needs
 *        them, so a window never receives high-rate events (AudioLevel at 30 Hz) it has no use for, and a query
 *        mounted in several components still produces one listener and one refetch per event. The subscribe
 *        function is injectable so the bridge is tested without Tauri.
 * WHERE: Started once per QueryClient by app/providers.tsx; queries declare their events through
 *        hooks/use-echo-query.ts.
 */
import type { QueryClient, QueryMeta } from "@tanstack/react-query";
import { subscribeEchoEvent, type EchoEventName, type Unsubscribe } from "./echo-events";

export type SubscribeEvent = (name: EchoEventName, handler: () => void) => Unsubscribe;

interface Listener {
  users: number;
  readonly stop: Unsubscribe;
}

/** The part of a cached query this bridge reads (the cache hands out queries with `any` data types). */
interface MetaCarrier {
  readonly meta: QueryMeta | undefined;
}

function eventsOf(query: MetaCarrier): readonly EchoEventName[] {
  return query.meta?.invalidatedBy ?? [];
}

export function startQueryInvalidation(
  client: QueryClient,
  subscribe: SubscribeEvent = subscribeEchoEvent,
): Unsubscribe {
  const cache = client.getQueryCache();
  const listeners = new Map<EchoEventName, Listener>();

  const invalidate = (name: EchoEventName): void => {
    void client.invalidateQueries({ predicate: (query) => eventsOf(query).includes(name) });
  };

  const track = (query: MetaCarrier): void => {
    for (const name of new Set(eventsOf(query))) {
      const listener = listeners.get(name);
      if (listener === undefined) {
        listeners.set(name, {
          users: 1,
          stop: subscribe(name, () => {
            invalidate(name);
          }),
        });
      } else {
        listener.users += 1;
      }
    }
  };

  const untrack = (query: MetaCarrier): void => {
    for (const name of new Set(eventsOf(query))) {
      const listener = listeners.get(name);
      if (listener === undefined) {
        continue;
      }
      listener.users -= 1;
      if (listener.users === 0) {
        listener.stop();
        listeners.delete(name);
      }
    }
  };

  for (const query of cache.getAll()) {
    track(query);
  }
  const stopCache = cache.subscribe((event) => {
    if (event.type === "added") {
      track(event.query);
    } else if (event.type === "removed") {
      untrack(event.query);
    }
  });

  return () => {
    stopCache();
    for (const listener of listeners.values()) {
      listener.stop();
    }
    listeners.clear();
  };
}
