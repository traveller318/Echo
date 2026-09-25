/**
 * SOURCE OF TRUTH KEYWORDS: dashboardLayout, DashboardLayout, SummaryMetric, ActivityMetric, metric emphasis layout, hero primary secondary
 * WHAT:  Sorts the registry's metrics into the 04 §5 Dashboard layout: `hero` and `primary` summary cards, the
 *        `activity` series (chart cards) and the `secondary` small cards, each in registry order, with the
 *        aggregate or day count every card reads; `summaryValue` looks a card's number up in a MetricsSummary.
 * WHY:   The page lays itself out from MetricSpec.query and .emphasis, never from a metric id (05 decision log), so
 *        adding a metric is a Rust registry entry with no change here. An activity series always goes to the
 *        chart row, whatever its emphasis, because it is drawn, not printed.
 * WHERE: routes/dashboard/index.tsx.
 */
import type { MetricAggregate, MetricSpec, MetricsSummary } from "@/bindings";

export interface SummaryMetric {
  readonly spec: MetricSpec;
  readonly aggregate: MetricAggregate;
}

export interface ActivityMetric {
  readonly spec: MetricSpec;
  readonly days: number;
}

export interface DashboardLayout {
  readonly hero: readonly SummaryMetric[];
  readonly primary: readonly SummaryMetric[];
  readonly activity: readonly ActivityMetric[];
  readonly secondary: readonly SummaryMetric[];
}

export function dashboardLayout(metrics: readonly MetricSpec[]): DashboardLayout {
  const hero: SummaryMetric[] = [];
  const primary: SummaryMetric[] = [];
  const activity: ActivityMetric[] = [];
  const secondary: SummaryMetric[] = [];
  const rows: Readonly<Record<MetricSpec["emphasis"], SummaryMetric[]>> = { hero, primary, secondary };
  for (const spec of metrics) {
    if (spec.query.kind === "activity") {
      activity.push({ spec, days: spec.query.days });
    } else {
      rows[spec.emphasis].push({ spec, aggregate: spec.query.aggregate });
    }
  }
  return { hero, primary, activity, secondary };
}

/** The value of `aggregate` in `summary`; null while it loads or when there is nothing to compute it from. */
export function summaryValue(summary: MetricsSummary | undefined, aggregate: MetricAggregate): number | null {
  return summary?.values.find((value) => value.aggregate === aggregate)?.value ?? null;
}
