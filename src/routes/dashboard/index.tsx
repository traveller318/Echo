/**
 * SOURCE OF TRUTH KEYWORDS: DashboardPage, dashboard route, dashboard page, hero stat, stat cards, activity chart, recent takes, metrics range
 * WHAT:  The Dashboard (04 §5): a range picker in the header, the hero stat card (time saved), the primary stat
 *        cards (words, transcriptions, speaking speed), the activity chart, the small cards (median latency,
 *        streak) and the five most recent takes with the shared take actions.
 * WHY:   The layout comes from the registry's metric entries (dashboardLayout: query + emphasis), so a new metric is
 *        a Rust registry entry and no change here. Every number is computed by Rust from the kept takes
 *        (metrics_summary / metrics_activity) and refreshed by MetricsChanged, never polled (02 §4.4); the page
 *        holds only UI state (the picked range in the dashboard store, the take awaiting a delete confirmation in useTakeInspector). While a
 *        newly picked range loads the previous numbers stay; the first read shows an indeterminate bar only after
 *        --delay-loading (ProgressBar), so a fast read never flashes. A failed read offers "Try again".
 * WHERE: Lazy-loaded by app/routes.tsx for the `dashboard` nav entry (app/nav-page.ts).
 */
import { useMemo } from "react";
import type { NavPageProps } from "@/app/nav-page";
import { EmptyState, NavIcon, Page, ProgressBar, TakeOverlays, useTakeInspector } from "@/components/global";
import { Button } from "@/components/ui";
import { useMetricsSummary, useRegistryView } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";
import { setMetricsRange, useDashboardStore } from "@/stores";
import { ActivityCard } from "./_components/ActivityCard";
import { dashboardLayout, summaryValue } from "./_components/dashboard-layout";
import { MetricCard } from "./_components/MetricCard";
import { RangeSelect } from "./_components/RangeSelect";
import { RecentTakesCard } from "./_components/RecentTakesCard";
import { StatGrid } from "./_components/StatGrid";

/** Takes in the recent list (04 §5 "last 5"). */
const RECENT_TAKES = 5;

export default function DashboardPage({ nav }: NavPageProps) {
  const { metrics } = useRegistryView();
  const layout = useMemo(() => dashboardLayout(metrics), [metrics]);
  const range = useDashboardStore((state) => state.range);
  const summary = useMetricsSummary(range);
  const takes = useTakeInspector();
  const data = summary.data;

  let numbers;
  if (summary.isError) {
    const copy = describeAppError(toAppError(summary.error));
    numbers = (
      <EmptyState
        icon={<NavIcon icon={nav.icon} />}
        title={copy.title}
        body={copy.body}
        action={
          <Button
            onClick={() => {
              void summary.refetch();
            }}
          >
            Try again
          </Button>
        }
      />
    );
  } else if (data === undefined) {
    numbers = <ProgressBar value={null} aria-label="Loading your numbers" />;
  } else {
    numbers = (
      <div data-slot="dashboard-metrics" aria-busy={summary.isPlaceholderData} className="flex flex-col gap-4">
        {layout.hero.map((metric) => (
          <MetricCard key={metric.spec.id} metric={metric} value={summaryValue(data, metric.aggregate)} size="hero" />
        ))}
        {layout.primary.length === 0 ? null : (
          <StatGrid columns={layout.primary.length}>
            {layout.primary.map((metric) => (
              <MetricCard key={metric.spec.id} metric={metric} value={summaryValue(data, metric.aggregate)} />
            ))}
          </StatGrid>
        )}
        {layout.activity.map((metric) => (
          <ActivityCard key={metric.spec.id} metric={metric} />
        ))}
        {layout.secondary.length === 0 ? null : (
          <StatGrid columns={layout.secondary.length}>
            {layout.secondary.map((metric) => (
              <MetricCard key={metric.spec.id} metric={metric} value={summaryValue(data, metric.aggregate)} />
            ))}
          </StatGrid>
        )}
      </div>
    );
  }

  return (
    <Page title={nav.label} actions={<RangeSelect value={range} onChange={setMetricsRange} />}>
      {numbers}
      <RecentTakesCard limit={RECENT_TAKES} inspector={takes} />
      <TakeOverlays inspector={takes} />
    </Page>
  );
}
