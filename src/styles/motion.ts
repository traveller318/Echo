/**
 * SOURCE OF TRUTH KEYWORDS: motion springs, SPRINGS, pillEnter, pillMorph, pillExit, press, transitionFor, reduced motion fade, MissingDesignTokenError
 * WHAT:  The four named springs of docs/04 §3.7 (the only place their numbers live) and `transitionFor(name,
 *        reducedMotion)`, which returns the spring, or under reduced motion an opacity-friendly tween timed by the
 *        `--duration-base` / `--ease-standard` tokens read from CSS.
 * WHY:   Springs are motion (JS) values, not CSS, so they cannot live in tokens.css; this file is their token
 *        file. The reduced-motion fade must not invent a second duration, so it reads the CSS tokens at call time
 *        and fails loudly (MissingDesignTokenError) if the stylesheet is missing instead of guessing a value.
 * WHERE: The pill (step 15) and any pressable surface: `transition={transitionFor("pillMorph", reduced)}` with
 *        `reduced` from motion's useReducedMotion(); tested in motion.test.ts.
 */
import type { Transition } from "motion/react";

export const SPRINGS = {
  pillEnter: { type: "spring", stiffness: 420, damping: 32, mass: 0.9 },
  pillMorph: { type: "spring", stiffness: 500, damping: 38, mass: 1 },
  pillExit: { type: "spring", stiffness: 380, damping: 36, mass: 1 },
  press: { type: "spring", stiffness: 700, damping: 40, mass: 1 },
} as const satisfies Readonly<Record<string, Transition>>;

export type SpringName = keyof typeof SPRINGS;

/** A design token the stylesheet should define is missing or unreadable. */
export class MissingDesignTokenError extends Error {
  constructor(token: string, value: string) {
    super(`Design token ${token} is missing or unreadable (got "${value}"). Is globals.css loaded?`);
    this.name = "MissingDesignTokenError";
  }
}

const REDUCED_DURATION_TOKEN = "--duration-base";
const REDUCED_EASE_TOKEN = "--ease-standard";
const MS_PER_SECOND = 1000;
const CUBIC_BEZIER = /^cubic-bezier\(([^)]+)\)$/;
const BEZIER_POINTS = 4;

function readToken(token: string, root: Element): string {
  return getComputedStyle(root).getPropertyValue(token).trim();
}

/** A CSS time token (`200ms`, `0.2s`) in seconds, the unit motion takes. */
export function readDurationToken(token: string, root: Element = document.documentElement): number {
  const value = readToken(token, root);
  const match = /^(\d*\.?\d+)(ms|s)$/.exec(value);
  const amount = match?.[1];
  const unit = match?.[2];
  if (amount === undefined || unit === undefined) {
    throw new MissingDesignTokenError(token, value);
  }
  const number = Number(amount);
  return unit === "ms" ? number / MS_PER_SECOND : number;
}

/** A `cubic-bezier(…)` token as the four control points motion takes. */
export function readEaseToken(
  token: string,
  root: Element = document.documentElement,
): [number, number, number, number] {
  const value = readToken(token, root);
  const points = CUBIC_BEZIER.exec(value)?.[1]?.split(",").map((point) => Number(point.trim()));
  if (points?.length !== BEZIER_POINTS || points.some((point) => Number.isNaN(point))) {
    throw new MissingDesignTokenError(token, value);
  }
  const [x1 = 0, y1 = 0, x2 = 0, y2 = 0] = points;
  return [x1, y1, x2, y2];
}

/** The spring for `name`, or the token-timed fade that replaces springs under reduced motion (04 §3.7). */
export function transitionFor(
  name: SpringName,
  reducedMotion: boolean,
  root: Element = document.documentElement,
): Transition {
  if (!reducedMotion) {
    return SPRINGS[name];
  }
  return {
    type: "tween",
    duration: readDurationToken(REDUCED_DURATION_TOKEN, root),
    ease: readEaseToken(REDUCED_EASE_TOKEN, root),
  };
}
