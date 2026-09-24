/**
 * SOURCE OF TRUTH KEYWORDS: hooks barrel, useEchoQuery, useEchoEvent, useEchoMutation, useRegistryView, data hooks
 * WHAT:  Barrel for src/hooks: the data hooks every window uses to read commands, run writes, follow Rust events
 *        and read the registry.
 * WHY:   One import path (`@/hooks`) for the data layer keeps components free of TanStack and Tauri details.
 * WHERE: Imported by the app shell, routes and (later) the pill.
 */
export { useEchoEvent, type UseEchoEventOptions } from "./use-echo-event";
export { useEchoMutation, type UseEchoMutationOptions } from "./use-echo-mutation";
export { echoQueryOptions, useEchoQuery, type EchoQuery, type UseEchoQueryOptions } from "./use-echo-query";
export {
  MissingRegistryError,
  REGISTRY_QUERY,
  RegistryContext,
  sortNavItems,
  useNavItems,
  useRegistryQuery,
  useRegistryView,
} from "./use-registry";
