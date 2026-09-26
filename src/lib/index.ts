/**
 * SOURCE OF TRUTH KEYWORDS: lib barrel, lib index, shared frontend helpers, exports
 * WHAT:  Barrel for src/lib: re-exports every shared, framework-agnostic frontend helper.
 * WHY:   One import path (`@/lib`) for helpers keeps call sites stable when files move inside lib/.
 * WHERE: Imported by window bootstraps, the app shell, hooks, routes and components.
 */
export {
  CommandError,
  describeAppError,
  describeTakeFailure,
  inlineAppError,
  isAppError,
  isAppErrorCode,
  toAppError,
  type AppErrorAction,
  type AppErrorActionId,
  type AppErrorCode,
  type AppErrorCopy,
  type AppErrorOf,
} from "./app-error";
export {
  APP_ERROR_ACTION_TARGETS,
  performAppErrorAction,
  type AppErrorActionTarget,
  type PerformAppErrorActionOptions,
} from "./app-error-actions";
export { applyAppearance, syncAppearance, type SyncAppearanceOptions } from "./appearance";
export { levelFromRms, smoothLevel } from "./audio-level";
export { cn, THEME_SCALE } from "./cn";
export { runCommand, type CommandResult } from "./command";
export { adoptCspStyleNonce, CSP_NONCE_SELECTOR } from "./csp-nonce";
export {
  ECHO_EVENT_NAMES,
  subscribeEchoEvent,
  type EchoEventName,
  type EchoEventPayload,
  type Unsubscribe,
} from "./echo-events";
export {
  formatBytes,
  formatClock,
  formatCount,
  formatDays,
  formatDuration,
  formatLanguage,
  formatMetricValue,
  formatMilliseconds,
  formatMinutes,
  formatSeconds,
  formatSettingInt,
  formatSettingUnit,
  formatTakeTime,
  formatWords,
  formatWpm,
  MISSING_VALUE,
  NUMERIC_CLASS,
} from "./format";
export { MissingRootElementError, mountRoot, ROOT_ELEMENT_ID } from "./mount-root";
export { createEchoQueryClient, type EchoQueryMeta } from "./query-client";
export { startQueryInvalidation, type SubscribeEvent } from "./query-invalidation";
