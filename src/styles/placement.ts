/**
 * SOURCE OF TRUTH KEYWORDS: placement tokens, POPOVER_SIDE_OFFSET, TOAST_DURATION_MS, TOAST_LIMIT, JS design tokens, Radix numeric props
 * WHAT:  Design values that JavaScript APIs take as plain numbers (docs/04 §3.10 "JS tokens"): the gap between a
 *        trigger and its floating content, how long a toast stays up and how many toasts stack at once.
 * WHY:   Radix positioning and toast timers take numbers, not CSS lengths, so these cannot live in tokens.css;
 *        like the springs in motion.ts, this file is their only source and docs/04 lists them.
 * WHERE: components/ui Tooltip and Select (`sideOffset`), components/ui Toast (`duration`), stores/toast-store.ts
 *        (`TOAST_LIMIT`).
 */

/** Pixels between a trigger and its tooltip or select list. */
export const POPOVER_SIDE_OFFSET = 6;

/** Milliseconds a toast stays visible before it dismisses itself. */
export const TOAST_DURATION_MS = 5000;

/** Toasts visible at once; a newer toast replaces the oldest beyond this. */
export const TOAST_LIMIT = 3;
