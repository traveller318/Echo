/**
 * SOURCE OF TRUTH KEYWORDS: components global barrel, reusable components, GlassSurface, EmptyState, ProgressBar, NavIcon, Page, DataList, TranscriptRow, SettingField, HotkeyInput
 * WHAT:  Barrel for every reusable app component (one folder each under components/global).
 * WHY:   Routes import `@/components/global` and never reach into a folder, so components can be reorganised
 *        inside their folders freely (03 §3). StatCard joins with its step.
 * WHERE: Imported by routes, the app shell and the pill.
 */
export {
  clampIndex,
  DataList,
  isNavigationKey,
  NAVIGATION_KEYS,
  nextActiveIndex,
  type DataListProps,
  type DataListRowState,
  type DataListSearchProps,
  type NavigationKey,
} from "./data-list";
export { EmptyState, type EmptyStateProps } from "./empty-state";
export { GLASS_SURFACE_VARIANTS, GlassSurface, type GlassSurfaceProps, type GlassSurfaceVariant } from "./glass-surface";
export {
  acceleratorKeys,
  formatAccelerator,
  HotkeyInput,
  isBindableChord,
  mainKeyToken,
  MODIFIER_TOKENS,
  modifierToken,
  orderModifiers,
  useHotkeyCapture,
  type HotkeyCapture,
  type HotkeyCaptureHint,
  type HotkeyInputProps,
  type ModifierToken,
} from "./hotkey-input";
export { NavIcon, type NavIconProps } from "./nav-icon";
export { Page, type PageProps } from "./page";
export { ProgressBar, progressFraction, type ProgressBarProps } from "./progress-bar";
export {
  optionLabel,
  SettingField,
  useSettingForm,
  type SettingControlProps,
  type SettingFieldProps,
  type SettingForm,
} from "./setting-field";
export {
  TRANSCRIPT_STATUS_LOOK,
  TranscriptRow,
  TranscriptStatusBadge,
  transcriptPlaceholder,
  type BadgeVariant,
  type TranscriptRowProps,
  type TranscriptStatusBadgeProps,
  type TranscriptStatusLook,
} from "./transcript-row";
