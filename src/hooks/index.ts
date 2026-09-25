/**
 * SOURCE OF TRUTH KEYWORDS: hooks barrel, useEchoQuery, useEchoInfiniteQuery, useEchoEvent, useEchoMutation, useRegistryView, useSessionView, useHistoryList, useTranscriptActions, data hooks
 * WHAT:  Barrel for src/hooks: the data hooks every window uses to read commands (single and paged), run writes,
 *        follow Rust events, read the registry, follow the current take and read and act on History, plus the
 *        timing hooks for delayed indicators and live counters.
 * WHY:   One import path (`@/hooks`) for the data layer keeps components free of TanStack and Tauri details.
 * WHERE: Imported by the app shell, routes and (later) the pill.
 */
export { useDelayedFlag } from "./use-delayed-flag";
export { useEchoEvent, type UseEchoEventOptions } from "./use-echo-event";
export {
  echoInfiniteQueryOptions,
  useEchoInfiniteQuery,
  type EchoInfiniteQuery,
  type UseEchoInfiniteQueryResult,
} from "./use-echo-infinite-query";
export { useEchoMutation, type UseEchoMutationOptions } from "./use-echo-mutation";
export { echoQueryOptions, useEchoQuery, type EchoQuery, type UseEchoQueryOptions } from "./use-echo-query";
export {
  HISTORY_EVENTS,
  HISTORY_PAGE_SIZE,
  historyListQuery,
  transcriptQuery,
  useHistoryList,
  useTranscript,
  useTranscriptActions,
  type TranscriptActions,
} from "./use-history";
export { useRunningClock, type RunningClockOptions } from "./use-running-clock";
export { useSessionView, type UseSessionViewOptions } from "./use-session-view";
export {
  MissingRegistryError,
  REGISTRY_QUERY,
  RegistryContext,
  sortNavItems,
  useNavItems,
  useRegistryQuery,
  useRegistryView,
} from "./use-registry";
