/**
 * SOURCE OF TRUTH KEYWORDS: ActivityCard, activity card, words per day card, activity summary, activity loading
 * WHAT:  A glass card for one registry activity metric: its label as the heading, a one-line summary of the window
 *        (words and active days), the ActivityChart, and the metric's help sentence; an indeterminate bar while it
 *        loads and a retry when the read fails.
 * WHY:   The series is its own query (useMetricsActivity, refreshed by MetricsChanged), so the chart loads and
 *        fails on its own without holding back the stat cards. The chart area keeps its --chart-height while
 *        loading or failed, so the page never jumps when the bars arrive. The window's length is the registry
 *        entry's `days`, never a number of the UI's. The chart (Recharts, the heaviest code in the window) is
 *        loaded lazily with the same placeholder as its data, so the stat cards paint before it is parsed.
 * WHERE: routes/dashboard/index.tsx (one per activity metric).
 */
import { lazy, Suspense, useId } from "react";
import { EmptyState, GlassSurface, ProgressBar } from "@/components/global";
import { Button } from "@/components/ui";
import { useMetricsActivity } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";
import { formatDays, formatWords, NUMERIC_CLASS } from "@/lib/format";
import { cn } from "@/lib/cn";
import type { ActivityMetric } from "./dashboard-layout";

const ActivityChart = lazy(() => import("./ActivityChart"));

export interface ActivityCardProps {
  readonly metric: ActivityMetric;
}

export function ActivityCard({ metric }: ActivityCardProps) {
  const headingId = useId();
  const activity = useMetricsActivity(metric.days);
  const days = activity.data;
  const words = days?.reduce((sum, day) => sum + day.words, 0) ?? 0;
  const active = days?.filter((day) => day.transcriptions > 0).length ?? 0;
  const summary =
    days === undefined ? null : active === 0 ? "No takes yet" : `${formatWords(words)} on ${formatDays(active)}`;

  return (
    <GlassSurface role="group" aria-labelledby={headingId} data-metric={metric.spec.id} className="flex flex-col gap-4 p-5">
      <header className="flex items-baseline justify-between gap-3">
        <h2 id={headingId} className="text-title3 text-fg">
          {metric.spec.label}
        </h2>
        {summary === null ? null : <p className={cn("text-footnote text-fg-secondary", NUMERIC_CLASS)}>{summary}</p>}
      </header>
      {activity.isError ? (
        <EmptyState
          className="h-chart p-0"
          title={describeAppError(toAppError(activity.error)).title}
          titleAs="p"
          action={
            <Button
              onClick={() => {
                void activity.refetch();
              }}
            >
              Try again
            </Button>
          }
        />
      ) : days === undefined ? (
        <ChartPlaceholder />
      ) : (
        <Suspense fallback={<ChartPlaceholder />}>
          <ActivityChart days={days} label={`${metric.spec.label}: ${summary ?? ""}`} />
        </Suspense>
      )}
      <p className="text-footnote text-fg-secondary">{metric.spec.help}</p>
    </GlassSurface>
  );
}

/** Holds the chart's height while its data or its code loads. */
function ChartPlaceholder() {
  return (
    <div className="flex h-chart items-center">
      <ProgressBar value={null} aria-label="Loading your activity" />
    </div>
  );
}
