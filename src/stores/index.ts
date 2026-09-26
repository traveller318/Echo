/**
 * SOURCE OF TRUTH KEYWORDS: stores barrel, zustand stores, UI state only, toast store, dashboard store, shell store
 * WHAT:  Barrel for src/stores: every Zustand store of UI-only state.
 * WHY:   Domain state stays in Rust and reaches the UI through commands and events (root CLAUDE.md §7); stores
 *        hold only what the UI itself owns (toasts, open panels, filters). One import path keeps that boundary
 *        visible.
 * WHERE: Imported by the app shell, hooks and routes.
 */
export { DEFAULT_METRICS_RANGE, setMetricsRange, useDashboardStore } from "./dashboard-store";
export { toggleSidebar, useShellStore } from "./shell-store";
export {
  dismissToast,
  showAppErrorToast,
  showToast,
  useToastStore,
  type ToastEntry,
  type ToastEntryAction,
  type ToastInput,
  type ToastTone,
} from "./toast-store";
