/**
 * SOURCE OF TRUTH KEYWORDS: createEchoQueryClient, QueryClient defaults, EchoQueryMeta, invalidatedBy, never polled, networkMode always, TanStack Query
 * WHAT:  `createEchoQueryClient()` builds the TanStack QueryClient every window uses, and `EchoQueryMeta` types the
 *        `meta` each query may carry (`invalidatedBy`: the Rust events that make its data stale).
 * WHY:   Command reads are cached by TanStack Query but kept fresh only by Rust events (02 §2.4, §4.4), never by
 *        polling or guessing: data never goes stale by time (`staleTime: Infinity`), and window focus, reconnect
 *        and intervals never refetch. `networkMode: "always"` because every "request" is local IPC: TanStack's
 *        default would pause all queries while Windows reports no network, and Echo must work fully offline.
 *        Failures are local and deterministic, so nothing is retried behind the user's back; the UI offers retry.
 * WHERE: app/providers.tsx (main window); later the pill and tests. The meta is read by lib/query-invalidation.ts
 *        and written by hooks/use-echo-query.ts.
 */
import { QueryClient } from "@tanstack/react-query";
import type { EchoEventName } from "./echo-events";

/** What an Echo query declares about itself in TanStack `meta`. */
export interface EchoQueryMeta extends Record<string, unknown> {
  /** Rust events after which this query's data is stale and is fetched again. */
  readonly invalidatedBy?: readonly EchoEventName[];
}

declare module "@tanstack/react-query" {
  interface Register {
    queryMeta: EchoQueryMeta;
  }
}

export function createEchoQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: Number.POSITIVE_INFINITY,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
        refetchInterval: false,
        retry: false,
        networkMode: "always",
      },
      mutations: {
        retry: false,
        networkMode: "always",
      },
    },
  });
}
