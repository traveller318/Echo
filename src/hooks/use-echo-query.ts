/**
 * SOURCE OF TRUTH KEYWORDS: useEchoQuery, echoQueryOptions, EchoQuery, command read, TanStack useQuery, invalidatedBy, event-driven refresh, never polled
 * WHAT:  `echoQueryOptions({ queryKey, command, invalidatedBy })` turns a generated command read into TanStack query
 *        options; `useEchoQuery(query, { select, enabled, keepPrevious })` runs it. Data resolves to the command's output; a
 *        failure is a CommandError (read the AppError with `toAppError(query.error)`).
 * WHY:   Every command read goes through TanStack Query so components share one cached result per key, and stays
 *        fresh only through the Rust events it names in `invalidatedBy` (lib/query-invalidation.ts refetches it),
 *        never by polling (02 §2.4). A query is declared once as a constant with echoQueryOptions and reused by
 *        hooks, prefetches and tests, so its key and events cannot drift between call sites.
 * WHERE: hooks/use-registry.ts (registry_get), use-settings.ts, use-history.ts, use-metrics.ts; later models reads.
 */
import { keepPreviousData, queryOptions, useQuery, type QueryKey, type UseQueryResult } from "@tanstack/react-query";
import { runCommand, type CommandResult } from "@/lib/command";
import type { EchoEventName } from "@/lib/echo-events";

export interface EchoQuery<T> {
  readonly queryKey: QueryKey;
  /** The generated command that reads the data, e.g. `commands.registryGet`. */
  readonly command: () => Promise<CommandResult<T>>;
  /** Rust events after which the data is stale. Omit for data that never changes while Echo runs. */
  readonly invalidatedBy?: readonly EchoEventName[];
}

export function echoQueryOptions<T>({ queryKey, command, invalidatedBy = [] }: EchoQuery<T>) {
  return queryOptions({
    queryKey,
    queryFn: () => runCommand(command),
    meta: { invalidatedBy },
  });
}

export interface UseEchoQueryOptions<T, S> {
  /** Derives what the component needs from the command output; re-runs only when the data changes. */
  readonly select?: (data: T) => S;
  /** Read only while true (default true). */
  readonly enabled?: boolean;
  /**
   * While a new key loads, keep showing the previous key's data (`isPlaceholderData` is true meanwhile), e.g. a
   * filter switch that should not flash an empty page.
   */
  readonly keepPrevious?: boolean;
}

export function useEchoQuery<T, S = T>(
  query: EchoQuery<T>,
  { select, enabled = true, keepPrevious = false }: UseEchoQueryOptions<T, S> = {},
): UseQueryResult<S> {
  return useQuery({
    ...echoQueryOptions(query),
    select,
    enabled,
    ...(keepPrevious ? { placeholderData: keepPreviousData } : {}),
  });
}
