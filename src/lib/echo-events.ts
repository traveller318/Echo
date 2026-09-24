/**
 * SOURCE OF TRUTH KEYWORDS: subscribeEchoEvent, EchoEventName, EchoEventPayload, typed event subscription, listen, unlisten race, Rust events
 * WHAT:  Typed access to the generated Rust → UI events: `EchoEventName` (every key of `events`),
 *        `EchoEventPayload<N>` (its payload type) and `subscribeEchoEvent(name, handler)`, which starts listening and
 *        returns a synchronous unsubscribe.
 * WHY:   Rust owns domain state and pushes it as typed events (root CLAUDE.md §7); the names and payloads come only
 *        from the generated bindings, so a renamed event fails tsc. Tauri's `listen` resolves its unlisten function
 *        asynchronously, and a React effect or a query cache can unsubscribe before that happens; this wrapper
 *        drops events after unsubscribe and releases the listener as soon as it exists, so no listener leaks and
 *        no handler runs late. A failed subscription is reported through `onError` (console by default), never
 *        thrown, because a missing live update must not take a window down.
 * WHERE: hooks/use-echo-event.ts (components), lib/query-invalidation.ts (event-driven query refresh).
 */
import type { EventCallback, UnlistenFn } from "@tauri-apps/api/event";
import { events, type AppError } from "@/bindings";
import { toAppError } from "./app-error";

type EchoEvents = typeof events;

/** Every event Rust can send, by its bindings key (e.g. `settingsChanged`). */
export type EchoEventName = keyof EchoEvents;

/** The payload type of one event. */
export type EchoEventPayload<N extends EchoEventName> = EchoEvents[N] extends {
  readonly listen: (handler: EventCallback<infer P>) => unknown;
}
  ? P
  : never;

export type Unsubscribe = () => void;

interface EventSource<P> {
  readonly listen: (handler: EventCallback<P>) => Promise<UnlistenFn>;
}

// A mapped type over the names lets `SOURCES[name]` keep the payload of `name` for a generic `name`.
const SOURCES: { readonly [N in EchoEventName]: EventSource<EchoEventPayload<N>> } = events;

/** Every event name, for code that must know them all (tests, diagnostics). */
export const ECHO_EVENT_NAMES = Object.keys(events).filter((name): name is EchoEventName =>
  Object.hasOwn(SOURCES, name),
);

function reportToConsole(error: AppError): void {
  console.error("Echo could not subscribe to an event", error);
}

export function subscribeEchoEvent<N extends EchoEventName>(
  name: N,
  handler: (payload: EchoEventPayload<N>) => void,
  onError: (error: AppError) => void = reportToConsole,
): Unsubscribe {
  // An object, not `let`s: both flags change inside callbacks, which control-flow narrowing cannot see.
  const state: { active: boolean; unlisten: UnlistenFn | null } = { active: true, unlisten: null };
  SOURCES[name]
    .listen((event) => {
      if (state.active) {
        handler(event.payload);
      }
    })
    .then(
      (unlisten) => {
        if (state.active) {
          state.unlisten = unlisten;
        } else {
          unlisten();
        }
      },
      (error: unknown) => {
        if (state.active) {
          onError(toAppError(error));
        }
      },
    );
  return () => {
    state.active = false;
    state.unlisten?.();
    state.unlisten = null;
  };
}
