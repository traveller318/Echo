/**
 * SOURCE OF TRUTH KEYWORDS: dashboard store, useDashboardStore, metrics range choice, DEFAULT_METRICS_RANGE, setMetricsRange, zustand
 * WHAT:  The Dashboard's UI state (Zustand): which MetricsRange the summary cards show, and `setMetricsRange`.
 * WHY:   The range is a view choice, not domain state (root CLAUDE.md §7): the numbers stay in Rust and the query
 *        cache, this only picks which window to ask for. A store rather than page state keeps the choice while the
 *        user visits other pages in the same window; it is not persisted, so each launch starts on all time
 *        (everything the history retention keeps).
 * WHERE: routes/dashboard (range picker and summary query).
 */
import { create } from "zustand";
import type { MetricsRange } from "@/bindings";

/** The range the Dashboard opens with. */
export const DEFAULT_METRICS_RANGE: MetricsRange = "all_time";

interface DashboardState {
  readonly range: MetricsRange;
}

export const useDashboardStore = create<DashboardState>()(() => ({ range: DEFAULT_METRICS_RANGE }));

export function setMetricsRange(range: MetricsRange): void {
  useDashboardStore.setState({ range });
}
