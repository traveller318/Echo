/**
 * SOURCE OF TRUTH KEYWORDS: placement tokens, POPOVER_SIDE_OFFSET, TOAST_DURATION_MS, TOAST_LIMIT, LIST_ROW_ESTIMATE, LIST_OVERSCAN, JS design tokens
 * WHAT:  Design values that JavaScript APIs take as plain numbers (docs/04 §3.10 "JS tokens"): the gap between a
 *        trigger and its floating content, how long a toast stays up, how many toasts stack at once, and the
 *        virtualized list's row estimate and overscan.
 * WHY:   Radix positioning, toast timers and TanStack Virtual take numbers, not CSS lengths, so these cannot live
 *        in tokens.css; like the springs in motion.ts, this file is their only source and docs/04 lists them.
 * WHERE: components/ui Tooltip and Select (`sideOffset`), components/ui Toast (`duration`), stores/toast-store.ts
 *        (`TOAST_LIMIT`), components/global/data-list (`LIST_ROW_ESTIMATE`, `LIST_OVERSCAN`).
 */

/** Pixels between a trigger and its tooltip or select list. */
export const POPOVER_SIDE_OFFSET = 6;

/** Milliseconds a toast stays visible before it dismisses itself. */
export const TOAST_DURATION_MS = 5000;

/** Toasts visible at once; a newer toast replaces the oldest beyond this. */
export const TOAST_LIMIT = 3;

/** Pixels a DataList row is assumed to take before it is measured (a two-line History row). */
export const LIST_ROW_ESTIMATE = 72;

/** DataList rows rendered beyond each edge of the viewport; also how close to the end the next page loads. */
export const LIST_OVERSCAN = 8;
