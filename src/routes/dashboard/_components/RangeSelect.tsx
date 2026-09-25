/**
 * SOURCE OF TRUTH KEYWORDS: RangeSelect, metrics range picker, dashboard window picker, today last 7 days last 30 days all time
 * WHAT:  The Dashboard's range picker: a Select of METRICS_RANGE_OPTIONS showing `value`; a pick that passes
 *        metricsRangeSchema is reported with `onChange`.
 * WHY:   The summary cards can cover today, the last 7 or 30 days or every kept take (MetricsRange); the activity
 *        chart and the streak have their own fixed windows. The Select hands back strings, so each pick is parsed
 *        against the declared schema before it is used (root CLAUDE.md §5).
 * WHERE: routes/dashboard/index.tsx (Page `actions` slot).
 */
import type { MetricsRange } from "@/bindings";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui";
import { METRICS_RANGE_OPTIONS, parseMetricsRange } from "./metrics-range";

export interface RangeSelectProps {
  readonly value: MetricsRange;
  readonly onChange: (range: MetricsRange) => void;
}

export function RangeSelect({ value, onChange }: RangeSelectProps) {
  return (
    <Select
      value={value}
      onValueChange={(next) => {
        const range = parseMetricsRange(next);
        if (range !== null) {
          onChange(range);
        }
      }}
    >
      <SelectTrigger aria-label="Show numbers for" className="w-auto">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {METRICS_RANGE_OPTIONS.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
