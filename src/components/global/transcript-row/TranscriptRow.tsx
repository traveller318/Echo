/**
 * SOURCE OF TRUTH KEYWORDS: TranscriptRow, history row, take row, two-line preview, take time, duration, word count, app name, status badge
 * WHAT:  One take as a list row: when it happened, how long it was, its word count and the app it went to on the
 *        first line with its status badge, and a two-line preview of its text below (or what its status says when
 *        it has no text).
 * WHY:   04 §5 History row (time, preview, duration, status badge), shared with the Dashboard's recent takes, so it
 *        is a DataList `row` slot with no behaviour of its own: actions and activation belong to the list. Numbers are
 *        tabular (04 §3.6); missing measurements are left out rather than shown as zero. `now` comes from the
 *        caller so a list formats every row against one clock.
 * WHERE: routes/history (DataList row slot); later routes/dashboard. Exported through components/global/index.ts.
 */
import type { TranscriptSummary } from "@/bindings";
import { cn } from "@/lib/cn";
import { formatDuration, formatTakeTime, formatWords, NUMERIC_CLASS } from "@/lib/format";
import { TranscriptStatusBadge } from "./TranscriptStatusBadge";
import { transcriptPlaceholder } from "./transcript-status";

export interface TranscriptRowProps {
  readonly take: TranscriptSummary;
  /** The clock the time is formatted against (ms). */
  readonly now: number;
  readonly className?: string;
}

export function TranscriptRow({ take, now, className }: TranscriptRowProps) {
  const meta = [
    take.duration_ms === null ? null : formatDuration(take.duration_ms),
    take.word_count === null || take.status !== "done" ? null : formatWords(take.word_count),
    take.app_name,
  ].filter((part): part is string => part !== null && part !== "");
  const preview = take.preview?.trim() ?? "";
  return (
    <div data-slot="transcript-row" className={cn("flex min-w-0 flex-col gap-1", className)}>
      <div className="flex min-w-0 items-center gap-2 text-footnote text-fg-secondary">
        <time dateTime={new Date(take.created_at).toISOString()} className={cn("shrink-0 text-fg", NUMERIC_CLASS)}>
          {formatTakeTime(take.created_at, now)}
        </time>
        {meta.length === 0 ? null : <span className={cn("min-w-0 truncate", NUMERIC_CLASS)}>{meta.join(" · ")}</span>}
        <TranscriptStatusBadge status={take.status} className="ml-auto" />
      </div>
      <p className={cn("line-clamp-2 text-body", preview === "" ? "text-fg-tertiary" : "text-fg")}>
        {preview === "" ? transcriptPlaceholder(take) : preview}
      </p>
    </div>
  );
}
