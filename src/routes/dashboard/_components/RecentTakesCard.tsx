/**
 * SOURCE OF TRUTH KEYWORDS: RecentTakesCard, recent takes, last takes, dashboard recent list, show all history
 * WHAT:  A glass card with the newest `limit` takes as DataList rows (TranscriptRow), the shared take actions on each
 *        row (Copy, Retry, Delete), the detail sheet on activation (through the page's TakeInspector) and a
 *        "Show all" button that opens History.
 * WHY:   04 §5 "recent takes (last 5, DataList rows)": the same list, row, actions and sheet as History
 *        (components/global), so a take behaves identically on both pages. The rows refresh from HistoryChanged,
 *        TranscriptSaved and MetricsChanged (useRecentTakes), never by polling; times are formatted against the
 *        read's own clock, so "today" is judged once per refresh.
 * WHERE: routes/dashboard/index.tsx.
 */
import { useId } from "react";
import type { TranscriptSummary } from "@/bindings";
import { useOpenPage } from "@/app/shell/use-open-page";
import {
  DataList,
  EmptyState,
  GlassSurface,
  ProgressBar,
  TakeRowActions,
  TranscriptRow,
  type TakeInspector,
} from "@/components/global";
import { Button } from "@/components/ui";
import { useRecentTakes } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";

export interface RecentTakesCardProps {
  /** How many takes to show. */
  readonly limit: number;
  readonly inspector: TakeInspector;
}

export function RecentTakesCard({ limit, inspector }: RecentTakesCardProps) {
  const headingId = useId();
  const takes = useRecentTakes(limit);
  const openPage = useOpenPage();

  const empty = takes.isPending ? (
    <ProgressBar value={null} aria-label="Loading your recent takes" />
  ) : takes.isError ? (
    <EmptyState
      title={describeAppError(toAppError(takes.error)).title}
      titleAs="p"
      action={
        <Button
          onClick={() => {
            void takes.refetch();
          }}
        >
          Try again
        </Button>
      }
    />
  ) : (
    <EmptyState
      titleAs="p"
      title="No takes yet"
      body="Press your dictation hotkey and speak. Your newest takes show up here."
    />
  );

  return (
    <GlassSurface role="group" aria-labelledby={headingId} className="flex flex-col gap-3 p-5">
      <header className="flex items-center justify-between gap-3">
        <h2 id={headingId} className="text-title3 text-fg">
          Recent takes
        </h2>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            openPage("history");
          }}
        >
          Show all
        </Button>
      </header>
      <DataList<TranscriptSummary>
        label="Recent takes"
        items={takes.data ?? []}
        getKey={(take) => take.id}
        row={(take) => <TranscriptRow take={take} now={takes.dataUpdatedAt} />}
        actions={(take) => <TakeRowActions take={take} actions={inspector.actions} onDelete={inspector.askDelete} />}
        empty={empty}
        onActivate={(take) => {
          inspector.open(take.id);
        }}
      />
    </GlassSurface>
  );
}
