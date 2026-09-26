/**
 * SOURCE OF TRUTH KEYWORDS: useWindowFocused, window focus, document.hasFocus, Echo in front, foreground window, focus blur events
 * WHAT:  `useWindowFocused()` is true while this Echo window has keyboard focus (it is the foreground window) and
 *        false while another app does; it re-renders on the window's `focus` and `blur` events.
 * WHY:   Rust decides where a take's text goes from the window in front when the hotkey is pressed (a take
 *        rehearsal keeps it in Echo only while an Echo window is the foreground, pipeline/session/rehearsal.rs). A
 *        visible Echo window is not necessarily the foreground one (another monitor, a click elsewhere), so a
 *        surface that depends on it must say so instead of letting the text land somewhere unseen. Read through
 *        useSyncExternalStore so every caller agrees and nothing is copied into state.
 * WHERE: routes/onboarding (PracticeStep: the practice pad's ready state and the line under it).
 */
import { useSyncExternalStore } from "react";

function subscribe(onChange: () => void): () => void {
  window.addEventListener("focus", onChange);
  window.addEventListener("blur", onChange);
  return () => {
    window.removeEventListener("focus", onChange);
    window.removeEventListener("blur", onChange);
  };
}

function snapshot(): boolean {
  return document.hasFocus();
}

export function useWindowFocused(): boolean {
  return useSyncExternalStore(subscribe, snapshot);
}
