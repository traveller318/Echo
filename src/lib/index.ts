/**
 * SOURCE OF TRUTH KEYWORDS: lib barrel, lib index, shared frontend helpers, exports
 * WHAT:  Barrel for src/lib: re-exports every shared, framework-agnostic frontend helper.
 * WHY:   One import path (`@/lib`) for helpers keeps call sites stable when files move inside lib/.
 * WHERE: Imported by window bootstraps and, later, by routes and components.
 */
export {
  describeAppError,
  isAppError,
  isAppErrorCode,
  toAppError,
  type AppErrorAction,
  type AppErrorActionId,
  type AppErrorCode,
  type AppErrorCopy,
  type AppErrorOf,
} from "./app-error";
export { applyAppearance, syncAppearance, type SyncAppearanceOptions, type Unsubscribe } from "./appearance";
export { cn, THEME_SCALE } from "./cn";
export { adoptCspStyleNonce, CSP_NONCE_SELECTOR } from "./csp-nonce";
export { MissingRootElementError, mountRoot, ROOT_ELEMENT_ID } from "./mount-root";
