/**
 * SOURCE OF TRUTH KEYWORDS: placement tokens, POPOVER_SIDE_OFFSET, TOAST_DURATION_MS, TOAST_LIMIT, LIST_ROW_ESTIMATE, LIST_OVERSCAN, CHART_BAR_RADIUS, CHART_TICK_MARGIN, CHART_MIN_TICK_GAP, CHART_BAR_GAP, JS design tokens
 * WHAT:  Design values that JavaScript APIs take as plain numbers (docs/04 §3.10 "JS tokens"): the gap between a
 *        trigger and its floating content, how long a toast stays up, how many toasts stack at once, the
 *        virtualized list's row estimate and overscan, and the chart geometry Recharts takes (bar corner radius,
 *        gap between bars, axis label spacing).
 * WHY:   Radix positioning, toast timers, TanStack Virtual and Recharts take numbers, not CSS lengths, so these
 *        cannot live in tokens.css; like the springs in motion.ts, this file is their only source and docs/04
 *        lists them.
 * WHERE: components/ui Tooltip and Select (`sideOffset`), components/ui Toast (`duration`), stores/toast-store.ts
 *        (`TOAST_LIMIT`), components/global/data-list (`LIST_ROW_ESTIMATE`, `LIST_OVERSCAN`), the Dashboard's
 *        activity chart (`CHART_*`).
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

/** Pixels of rounding on a chart bar's top corners (Recharts clamps it to short bars). */
export const CHART_BAR_RADIUS = 4;

/** Gap between neighbouring chart bars, as a share of each bar's slot. */
export const CHART_BAR_GAP = "24%";

/** Pixels between a chart axis and its labels. */
export const CHART_TICK_MARGIN = 8;

/** Smallest gap in pixels between two axis labels; labels that would crowd closer are skipped. */
export const CHART_MIN_TICK_GAP = 32;
