/**
 * SOURCE OF TRUTH KEYWORDS: appearance sync, applyAppearance, syncAppearance, data-theme, data-transparency, data-backdrop, AppearanceChanged listener, appearance_get
 * WHAT:  `applyAppearance(view, root)` writes Rust's AppearanceView onto `<html>` as the data attributes the design
 *        tokens key off; `syncAppearance(root)` subscribes to AppearanceChanged, reads the view once, and keeps
 *        applying every later change. Resolves to the unsubscribe function.
 * WHY:   Rust owns appearance and pushes it (root CLAUDE.md §7); the UI keeps no copy, it only mirrors the view
 *        into three attributes (docs/04 §2). The listener is attached before the read, so no change can fall
 *        between them, and a read that returns after a newer event is dropped. The attribute maps are keyed by
 *        the generated union types, so a new Rust value fails tsc until it has an attribute here. If the read
 *        fails, the CSS defaults stay in force (follow Windows, token background), which is always legible, and
 *        the failure is reported through `onError`.
 * WHERE: Called by both window bootstraps (src/main.tsx, src/pill.tsx) before mounting React; tested in
 *        appearance.test.ts.
 */
import {
  commands,
  events,
  type AppError,
  type AppearanceView,
  type Backdrop,
  type ThemePreference,
  type Transparency,
} from "@/bindings";
import { toAppError } from "./app-error";

const THEME_ATTRIBUTE = "data-theme";
const TRANSPARENCY_ATTRIBUTE = "data-transparency";
const BACKDROP_ATTRIBUTE = "data-backdrop";

/** Attribute value per choice; `null` removes the attribute (follow Windows / full transparency). */
const THEME_VALUE: Readonly<Record<ThemePreference, string | null>> = {
  system: null,
  light: "light",
  dark: "dark",
};

const TRANSPARENCY_VALUE: Readonly<Record<Transparency, string | null>> = {
  full: null,
  reduced: "reduced",
};

const BACKDROP_VALUE: Readonly<Record<Backdrop, string>> = {
  mica: "mica",
  solid: "solid",
};

function setOrRemove(root: HTMLElement, name: string, value: string | null): void {
  if (value === null) {
    root.removeAttribute(name);
  } else {
    root.setAttribute(name, value);
  }
}

export function applyAppearance(view: AppearanceView, root: HTMLElement = document.documentElement): void {
  setOrRemove(root, THEME_ATTRIBUTE, THEME_VALUE[view.theme]);
  setOrRemove(root, TRANSPARENCY_ATTRIBUTE, TRANSPARENCY_VALUE[view.transparency]);
  setOrRemove(root, BACKDROP_ATTRIBUTE, BACKDROP_VALUE[view.backdrop]);
}

export type Unsubscribe = () => void;

export interface SyncAppearanceOptions {
  readonly root?: HTMLElement;
  /** Called when the subscription or the first read fails; the CSS defaults stay in force. */
  readonly onError?: (error: AppError) => void;
}

function reportToConsole(error: AppError): void {
  console.error("Echo could not read the window appearance", error);
}

export async function syncAppearance(options: SyncAppearanceOptions = {}): Promise<Unsubscribe> {
  const root = options.root ?? document.documentElement;
  const onError = options.onError ?? reportToConsole;
  // An object, not a `let`: the flag flips inside the listener, which control-flow narrowing cannot see.
  const seen = { change: false };
  let unsubscribe: Unsubscribe = () => undefined;
  try {
    unsubscribe = await events.appearanceChanged.listen((event) => {
      seen.change = true;
      applyAppearance(event.payload, root);
    });
    const result = await commands.appearanceGet();
    if (result.status === "error") {
      onError(result.error);
    } else if (!seen.change) {
      applyAppearance(result.data, root);
    }
  } catch (error: unknown) {
    onError(toAppError(error));
  }
  return unsubscribe;
}
