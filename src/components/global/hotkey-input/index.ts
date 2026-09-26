/**
 * SOURCE OF TRUTH KEYWORDS: hotkey-input barrel, HotkeyInput, ShortcutKeys, accelerator helpers, useHotkeyCapture
 * WHAT:  Public surface of the hotkey-input folder: the HotkeyInput field, its capture hook and the accelerator
 *        helpers (format, split, key tokens, the bindable-chord rule).
 * WHY:   Callers import `@/components/global`; the folder's files can move without touching them.
 * WHERE: components/global/index.ts.
 */
export {
  acceleratorKeys,
  formatAccelerator,
  isBindableChord,
  mainKeyToken,
  MODIFIER_TOKENS,
  modifierToken,
  orderModifiers,
  type ModifierToken,
} from "./accelerator";
export { HotkeyInput, type HotkeyInputProps } from "./HotkeyInput";
export { ShortcutKeys, type ShortcutKeysProps } from "./ShortcutKeys";
export { useHotkeyCapture, type HotkeyCapture, type HotkeyCaptureHint } from "./use-hotkey-capture";
