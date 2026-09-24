/**
 * SOURCE OF TRUTH KEYWORDS: useEchoEvent, typed event hook, Rust event subscription, useEffectEvent, listen in component
 * WHAT:  `useEchoEvent(name, handler, options)` calls `handler(payload)` for every Rust event `name` while the
 *        component is mounted (and `enabled` is not false).
 * WHY:   Components react to pushed state (SessionStateChanged, AudioLevel, ModelProgress) instead of polling
 *        (root CLAUDE.md §7). The handler goes through `useEffectEvent`, so it always sees the latest props
 *        without resubscribing on every render; the subscription itself (lib/echo-events.ts) handles the async
 *        listen/unlisten race and reports failures.
 * WHERE: The pill (SessionStateChanged, AudioLevel, step 15), Models progress (ModelProgress, step 21); data that
 *        a command reads should use useEchoQuery with `invalidatedBy` instead.
 */
import { useEffect, useEffectEvent } from "react";
import { subscribeEchoEvent, type EchoEventName, type EchoEventPayload } from "@/lib/echo-events";

export interface UseEchoEventOptions {
  /** Subscribe only while true (default true). */
  readonly enabled?: boolean;
}

export function useEchoEvent<N extends EchoEventName>(
  name: N,
  handler: (payload: EchoEventPayload<N>) => void,
  { enabled = true }: UseEchoEventOptions = {},
): void {
  const onEvent = useEffectEvent(handler);
  useEffect(() => {
    if (!enabled) {
      return undefined;
    }
    return subscribeEchoEvent(name, (payload) => {
      onEvent(payload);
    });
  }, [name, enabled]);
}
