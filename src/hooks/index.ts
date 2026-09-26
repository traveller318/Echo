/**
 * SOURCE OF TRUTH KEYWORDS: hooks barrel, useEchoQuery, useEchoInfiniteQuery, useEchoEvent, useEchoMutation, useRegistryView, useSessionView, useHistoryList, useMetricsSummary, useModels, useModelActions, useSpeechEngineStatus, useSettingValues, useSettingWrite, useOnboarding, useSessionRehearsal, useMicCheck, useAudioLevel, useWindowFocused, data hooks
 * WHAT:  Barrel for src/hooks: the data hooks every window uses to read commands (single and paged), run writes,
 *        follow Rust events, read the registry, follow the current take, read and act on History, read the
 *        Dashboard metrics, read and act on Models, read where the speech engine runs, read and act on Settings, read and
 *        finish onboarding, set the session rehearsal, run a microphone check with its live level, tell whether the window is in front, plus the timing
 *        hooks for delayed indicators and live counters.
 * WHY:   One import path (`@/hooks`) for the data layer keeps components free of TanStack and Tauri details.
 * WHERE: Imported by the app shell, routes and (later) the pill.
 */
export { useAudioLevel } from "./use-audio-level";
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
  recentTakesQuery,
  transcriptQuery,
  useHistoryList,
  useRecentTakes,
  useTranscript,
  useTranscriptActions,
  type TranscriptActions,
} from "./use-history";
export {
  METRICS_EVENTS,
  metricsActivityQuery,
  metricsSummaryQuery,
  useMetricsActivity,
  useMetricsSummary,
} from "./use-metrics";
export {
  isTerminalPhase,
  MODELS_EVENTS,
  MODELS_QUERY,
  useModelActions,
  useModels,
  useModelTransfers,
  type EngineRef,
  type ModelActions,
  type ModelManifestRef,
  type ModelTransfers,
} from "./use-models";
export { micCheckWindowMs, useMicCheck, type MicCheckRun } from "./use-mic-check";
export {
  ONBOARDING_EVENTS,
  ONBOARDING_QUERY,
  useCompleteOnboarding,
  useOnboarding,
  useRefreshOnWindowFocus,
} from "./use-onboarding";
export { useRunningClock, type RunningClockOptions } from "./use-running-clock";
export { useSessionRehearsal } from "./use-session-rehearsal";
export { useSessionView, type UseSessionViewOptions } from "./use-session-view";
export { useWindowFocused } from "./use-window-focused";
export {
  SPEECH_ENGINE_EVENTS,
  SPEECH_ENGINE_QUERY,
  useRemeasureAccelerator,
  useSpeechEngineStatus,
  type RemeasureAccelerator,
} from "./use-speech-engine";
export {
  AUDIO_DEVICES_QUERY,
  SETTINGS_AVAILABILITY_QUERY,
  SETTINGS_EVENTS,
  SETTINGS_QUERY,
  useAudioDevices,
  useSettingOptions,
  useSettingsAvailability,
  useSettingValues,
  useSettingWrite,
  type SettingChange,
  type SettingRowOptions,
  type SettingValues,
  type SettingWrite,
} from "./use-settings";
export {
  MissingRegistryError,
  REGISTRY_QUERY,
  RegistryContext,
  sortNavItems,
  useNavItems,
  useRegistryQuery,
  useRegistryView,
} from "./use-registry";
