/**
 * SOURCE OF TRUTH KEYWORDS: METRICS_RANGE_OPTIONS, metrics range labels, metricsRangeSchema, range picker copy, parseMetricsRange
 * WHAT:  The Dashboard's range choices in picker order with their labels (`METRICS_RANGE_OPTIONS`), the Zod schema
 *        a picked value must pass (`metricsRangeSchema`) and `parseMetricsRange(text)` (the range, or null).
 * WHY:   The label table is keyed by the generated MetricsRange union, so a new range in Rust fails tsc until it has
 *        copy here; the picker hands back plain strings, and every input is validated against a declared schema
 *        (root CLAUDE.md §5) before it becomes the store's range.
 * WHERE: routes/dashboard/_components/RangeSelect.tsx.
 */
import { z } from "zod";
import type { MetricsRange } from "@/bindings";

const LABELS: Readonly<Record<MetricsRange, string>> = {
  today: "Today",
  last_7_days: "Last 7 days",
  last_30_days: "Last 30 days",
  all_time: "All time",
};

const ORDER = ["today", "last_7_days", "last_30_days", "all_time"] as const satisfies readonly MetricsRange[];

export const METRICS_RANGE_OPTIONS: readonly { readonly value: MetricsRange; readonly label: string }[] = ORDER.map(
  (value) => ({ value, label: LABELS[value] }),
);

export const metricsRangeSchema = z.enum(ORDER);

export function parseMetricsRange(text: string): MetricsRange | null {
  const parsed = metricsRangeSchema.safeParse(text);
  return parsed.success ? parsed.data : null;
}

export function metricsRangeLabel(range: MetricsRange): string {
  return LABELS[range];
}
