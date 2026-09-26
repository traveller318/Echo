/**
 * SOURCE OF TRUTH KEYWORDS: useHotkeyCapture, hotkey capture, record shortcut, modifier-only capture, capture hint, key handlers, onCaptureChange
 * WHAT:  The capture state of a HotkeyInput: `start` begins listening, `onKeyDown` / `onKeyUp` read keys from the
 *        focused element, and a finished capture is reported once through `onCapture` with the accelerator text.
 *        `held` are the modifiers down right now (shown live), `hint` why the last attempt could not be used, and
 *        `cancel` stops without reporting. `onCaptureChange` hears true when capturing starts and false when it ends
 *        for any reason (a shortcut, Escape, blur, unmount).
 * WHY:   A main-key chord finishes on the main key's keydown, with the modifiers the event says are held (so keys
 *        pressed before focus still count). A modifier-only chord (Ctrl+Alt, the default dictation hotkey) has no
 *        main key, so it finishes when every modifier is up, from the largest set held together. Escape alone
 *        cancels and Tab alone leaves (keyboard users must be able to move on); every other key is swallowed while
 *        capturing, so Enter or Space cannot re-trigger the button. Key repeats are ignored. The start/end report is an
 *        effect on the capturing state, so every way a capture ends (including the field unmounting mid-capture) is
 *        reported exactly once; the caller decides what it means (Settings switches Echo's own hotkeys off meanwhile).
 * WHERE: HotkeyInput.tsx.
 */
import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import {
  formatAccelerator,
  isBindableChord,
  mainKeyToken,
  modifierToken,
  orderModifiers,
  type ModifierToken,
} from "./accelerator";

/** Why a capture attempt could not be used. */
export type HotkeyCaptureHint = "needs-modifier" | "needs-second-modifier" | "unsupported-key";

export interface HotkeyCapture {
  readonly capturing: boolean;
  /** Modifiers held right now, in accelerator order. */
  readonly held: readonly ModifierToken[];
  readonly hint: HotkeyCaptureHint | null;
  readonly start: () => void;
  readonly cancel: () => void;
  readonly onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
  readonly onKeyUp: (event: KeyboardEvent<HTMLElement>) => void;
}

function heldModifiers(event: KeyboardEvent<HTMLElement>): ModifierToken[] {
  const held: ModifierToken[] = [];
  if (event.ctrlKey) held.push("Ctrl");
  if (event.altKey) held.push("Alt");
  if (event.shiftKey) held.push("Shift");
  if (event.metaKey) held.push("Win");
  return held;
}

export function useHotkeyCapture(
  onCapture: (accelerator: string) => void,
  onCaptureChange?: (capturing: boolean) => void,
): HotkeyCapture {
  const [capturing, setCapturing] = useState(false);
  const [held, setHeld] = useState<readonly ModifierToken[]>([]);
  const [hint, setHint] = useState<HotkeyCaptureHint | null>(null);
  // The largest set of modifiers held together since the last key-up that emptied the set.
  const peak = useRef(new Set<ModifierToken>());
  const reportChange = useRef(onCaptureChange);
  useEffect(() => {
    reportChange.current = onCaptureChange;
  });
  useEffect(() => {
    if (!capturing) {
      return undefined;
    }
    reportChange.current?.(true);
    return () => {
      reportChange.current?.(false);
    };
  }, [capturing]);

  const reset = useCallback(() => {
    peak.current = new Set();
    setHeld([]);
  }, []);

  const start = useCallback(() => {
    reset();
    setHint(null);
    setCapturing(true);
  }, [reset]);

  const cancel = useCallback(() => {
    reset();
    setCapturing(false);
  }, [reset]);

  const finish = useCallback(
    (accelerator: string) => {
      reset();
      setHint(null);
      setCapturing(false);
      onCapture(accelerator);
    },
    [onCapture, reset],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLElement>) => {
      if (!capturing) {
        return;
      }
      const modifiers = heldModifiers(event);
      if (modifiers.length === 0 && event.code === "Escape") {
        event.preventDefault();
        cancel();
        return;
      }
      if (modifiers.length === 0 && event.code === "Tab") {
        cancel();
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      if (event.repeat) {
        return;
      }
      const modifier = modifierToken(event.code);
      if (modifier !== undefined) {
        peak.current.add(modifier);
        setHeld(orderModifiers([...modifiers, modifier]));
        setHint(null);
        return;
      }
      const key = mainKeyToken(event.code);
      if (key === undefined) {
        setHint("unsupported-key");
      } else if (isBindableChord(modifiers, key)) {
        finish(formatAccelerator(modifiers, key));
      } else {
        setHint("needs-modifier");
      }
    },
    [cancel, capturing, finish],
  );

  const onKeyUp = useCallback(
    (event: KeyboardEvent<HTMLElement>) => {
      if (!capturing) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      const modifier = modifierToken(event.code);
      if (modifier === undefined) {
        return;
      }
      const still = heldModifiers(event).filter((held) => held !== modifier);
      setHeld(still);
      if (still.length > 0) {
        return;
      }
      const chord = [...peak.current];
      peak.current = new Set();
      if (isBindableChord(chord)) {
        finish(formatAccelerator(chord));
      } else if (chord.length > 0) {
        setHint("needs-second-modifier");
      }
    },
    [capturing, finish],
  );

  return { capturing, held, hint, start, cancel, onKeyDown, onKeyUp };
}
