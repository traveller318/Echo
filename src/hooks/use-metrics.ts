/**
 * SOURCE OF TRUTH KEYWORDS: useMetricsSummary, useMetricsActivity, metricsSummaryQuery, metricsActivityQuery, METRICS_EVENTS, metrics_summary, metrics_activity, dashboard data
 * WHAT:  The Dashboard data layer: `metricsSummaryQuery(range)` / `useMetricsSummary(range)` (every registry
 *        summary metric over a MetricsRange) and `metricsActivityQuery(days)` / `useMetricsActivity(days)` (words and
 *        takes per local day, oldest first, every day present).
 * WHY:   Metrics are computed by Rust from the kept takes at every read (02 §7.4); the UI keeps no copy and never
 *        polls. Both reads are invalidated by MetricsChanged, which Rust sends after every take, retry, delete,
 *        retention sweep, typing-speed change and at local midnight (02 §4.4), so one event covers every reason the
 *        numbers can move. Keys carry the range / days, so switching the range caches each window separately.
 * WHERE: routes/dashboard (hero, stat cards, activity chart, small cards).
 */
import { commands, type ActivityDay, type MetricsRange, type MetricsSummary } from "@/bindings";
import type { EchoEventName } from "@/lib/echo-events";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";

/** Rust events after which any metrics read is stale. */
export const METRICS_EVENTS: readonly EchoEventName[] = ["metricsChanged"];

/** Every summary metric over `range`. */
export function metricsSummaryQuery(range: MetricsRange): EchoQuery<MetricsSummary> {
  return {
    queryKey: ["metrics", "summary", range],
    command: () => commands.metricsSummary({ range }),
    invalidatedBy: METRICS_EVENTS,
  };
}

/** Words and completed takes per local day for the `days` days ending today. */
export function metricsActivityQuery(days: number): EchoQuery<ActivityDay[]> {
  return {
    queryKey: ["metrics", "activity", days],
    command: () => commands.metricsActivity({ days }),
    invalidatedBy: METRICS_EVENTS,
  };
}

/** The summary over `range`; while a newly picked range loads, the previous range's numbers stay on screen. */
export function useMetricsSummary(range: MetricsRange) {
  return useEchoQuery(metricsSummaryQuery(range), { keepPrevious: true });
}

/** The activity series of `days` days (the registry activity metric's length). */
export function useMetricsActivity(days: number) {
  return useEchoQuery(metricsActivityQuery(days));
}
