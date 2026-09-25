/**
 * SOURCE OF TRUTH KEYWORDS: accelerator, formatAccelerator, acceleratorKeys, mainKeyToken, modifierToken, orderModifiers, isBindableChord, MODIFIER_TOKENS, keyboard event to shortcut
 * WHAT:  Turns captured keys into the accelerator text Echo stores (`Ctrl+Alt+Space`, `Ctrl+Alt`) and back into the
 *        keys a chip row shows: `modifierToken` / `mainKeyToken` name a KeyboardEvent.code, `formatAccelerator`
 *        joins modifiers (in the fixed order `orderModifiers` gives) and a main key, `acceleratorKeys` splits stored text, and
 *        `isBindableChord` says whether a capture may be saved.
 * WHY:   The combination stays text end to end and only the hotkey adapter parses it (types/hotkey.rs), so this
 *        writes exactly the spelling the adapter reads (adapters/hotkey/low_level_hook/chord.rs: case-insensitive,
 *        `+`-separated, the main key last, US-layout names for punctuation). Keys come from `event.code`, the
 *        physical key, because the adapter binds physical virtual keys whatever the layout. A main key needs a
 *        modifier (except F1–F24) and a modifier-only chord needs two or more, so a capture never takes a key the
 *        user types everywhere (the same rule the Hotkey error copy states). Lock keys and Escape are never keys:
 *        Escape cancels a capture and is Echo's fixed cancel key.
 * WHERE: HotkeyInput (components/global/hotkey-input) and its capture hook; onboarding's hotkey step (step 24).
 */

/** Modifier names in the order an accelerator lists them. */
export const MODIFIER_TOKENS = ["Ctrl", "Alt", "Shift", "Win"] as const;

export type ModifierToken = (typeof MODIFIER_TOKENS)[number];

const MODIFIER_CODES: Readonly<Record<string, ModifierToken>> = {
  ControlLeft: "Ctrl",
  ControlRight: "Ctrl",
  AltLeft: "Alt",
  AltRight: "Alt",
  ShiftLeft: "Shift",
  ShiftRight: "Shift",
  MetaLeft: "Win",
  MetaRight: "Win",
  OSLeft: "Win",
  OSRight: "Win",
};

/** Named keys whose KeyboardEvent.code differs from the adapter's spelling or needs no change. */
const NAMED_CODES: Readonly<Record<string, string>> = {
  Space: "Space",
  Enter: "Enter",
  NumpadEnter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Pause: "Pause",
  PrintScreen: "PrintScreen",
  Comma: "Comma",
  Period: "Period",
  Slash: "Slash",
  Semicolon: "Semicolon",
  Quote: "Quote",
  BracketLeft: "BracketLeft",
  BracketRight: "BracketRight",
  Backslash: "Backslash",
  Minus: "Minus",
  Equal: "Equal",
  Backquote: "Backquote",
  NumpadMultiply: "NumpadMultiply",
  NumpadAdd: "NumpadAdd",
  NumpadSubtract: "NumpadSubtract",
  NumpadDecimal: "NumpadDecimal",
  NumpadDivide: "NumpadDivide",
};

const LETTER = /^Key([A-Z])$/;
const DIGIT = /^Digit([0-9])$/;
const NUMPAD_DIGIT = /^Numpad([0-9])$/;
const FUNCTION_KEY = /^F([1-9]|1[0-9]|2[0-4])$/;

/** The modifier a KeyboardEvent.code is, if it is one. */
export function modifierToken(code: string): ModifierToken | undefined {
  return MODIFIER_CODES[code];
}

/** The main-key token of a KeyboardEvent.code in the adapter's spelling; undefined for a key Echo cannot bind. */
export function mainKeyToken(code: string): string | undefined {
  const letter = LETTER.exec(code)?.[1];
  if (letter !== undefined) {
    return letter;
  }
  const digit = DIGIT.exec(code)?.[1];
  if (digit !== undefined) {
    return digit;
  }
  if (NUMPAD_DIGIT.test(code) || FUNCTION_KEY.test(code)) {
    return code;
  }
  return NAMED_CODES[code];
}

/** Modifiers in accelerator order, each once. */
export function orderModifiers(modifiers: Iterable<ModifierToken>): ModifierToken[] {
  const held = new Set(modifiers);
  return MODIFIER_TOKENS.filter((modifier) => held.has(modifier));
}

/** The accelerator text for `modifiers` plus an optional main key: `Ctrl+Alt+Space`, `Ctrl+Alt`. */
export function formatAccelerator(modifiers: Iterable<ModifierToken>, key?: string): string {
  return [...orderModifiers(modifiers), ...(key === undefined ? [] : [key])].join("+");
}

/** The keys of a stored accelerator, for display as chips. */
export function acceleratorKeys(accelerator: string): string[] {
  return accelerator
    .split("+")
    .map((part) => part.trim())
    .filter((part) => part !== "");
}

/**
 * SOURCE OF TRUTH KEYWORDS: isBindableChord, hotkey capture rule, modifier-only chord, function key alone
 * WHAT:  True for a main key with at least one modifier, a function key (F1–F24) alone, or two or more modifiers
 *        alone.
 * WHY:   A plain letter or a lone modifier as a global hotkey would fire on everyday typing; the adapter itself
 *        refuses a modifier-only chord with fewer than two modifiers.
 * WHERE: useHotkeyCapture, before a capture is reported.
 */
export function isBindableChord(modifiers: Iterable<ModifierToken>, key?: string): boolean {
  const count = orderModifiers(modifiers).length;
  if (key === undefined) {
    return count >= 2;
  }
  return count >= 1 || FUNCTION_KEY.test(key);
}
