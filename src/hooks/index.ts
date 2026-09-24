/**
 * SOURCE OF TRUTH KEYWORDS: hooks barrel, useEchoQuery, useEchoEvent, useEchoMutation, useRegistryView, useSessionView, useDelayedFlag, useRunningClock, data hooks
 * WHAT:  Barrel for src/hooks: the data hooks every window uses to read commands, run writes, follow Rust events,
 *        read the registry and follow the current take, plus the timing hooks for delayed indicators and live
 *        counters.
 * WHY:   One import path (`@/hooks`) for the data layer keeps components free of TanStack and Tauri details.
 * WHERE: Imported by the app shell, routes and (later) the pill.
 */
export { useDelayedFlag } from "./use-delayed-flag";
export { useEchoEvent, type UseEchoEventOptions } from "./use-echo-event";
export { useEchoMutation, type UseEchoMutationOptions } from "./use-echo-mutation";
export { echoQueryOptions, useEchoQuery, type EchoQuery, type UseEchoQueryOptions } from "./use-echo-query";
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
