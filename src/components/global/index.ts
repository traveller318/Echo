/**
 * SOURCE OF TRUTH KEYWORDS: components global barrel, reusable components, GlassSurface, EmptyState, InlineNotice, ModelCard, ModelCardActions, ProgressBar, NavIcon, Page, DataList, TranscriptRow, SettingField, SettingRow, HotkeyInput, StatCard, MicCheckPanel, take actions
 * WHAT:  Barrel for every reusable app component (one folder each under components/global).
 * WHY:   Routes import `@/components/global` and never reach into a folder, so components can be reorganised
 *        inside their folders freely (03 §3). Components inside global/ import each other by folder, never
 *        through this barrel, so no folder depends on the whole set.
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
  ShortcutKeys,
  useHotkeyCapture,
  type HotkeyCapture,
  type HotkeyCaptureHint,
  type HotkeyInputProps,
  type ModifierToken,
  type ShortcutKeysProps,
} from "./hotkey-input";
export { InlineNotice, type InlineNoticeProps } from "./inline-notice";
export { MIC_VERDICT_LOOK, MicCheckPanel, type MicCheckPanelProps, type MicVerdictLook } from "./mic-check";
export {
  ALL_SECONDARY_MODEL_ACTIONS,
  engineKindLabel,
  isDeterminate,
  languagesSummary,
  ModelCard,
  ModelCardActions,
  modelActionPlan,
  modelStatusLook,
  needsNetwork,
  transferSummary,
  type ModelActionPlan,
  type ModelCardActionsProps,
  type ModelCardProps,
  type ModelStatusLook,
  type PrimaryModelAction,
  type SecondaryModelAction,
} from "./model-card";
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
export { SettingRow, SettingRowFor, type SettingRowForProps, type SettingRowProps } from "./setting-row";
export {
  StatCard,
  StatFigure,
  StatUnit,
  type StatCardProps,
  type StatCardSize,
  type StatFigureProps,
} from "./stat-card";
export {
  DeleteTakeDialog,
  TakeActionButton,
  takeAvailability,
  TakeOverlays,
  TakeRowActions,
  TranscriptSheet,
  useTakeInspector,
  type DeleteTakeDialogProps,
  type TakeActionButtonProps,
  type TakeAvailability,
  type TakeInspector,
  type TakeOverlaysProps,
  type TakeRowActionsProps,
  type TranscriptSheetProps,
} from "./take-actions";
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
