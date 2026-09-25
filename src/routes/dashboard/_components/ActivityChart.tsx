/**
 * SOURCE OF TRUTH KEYWORDS: ActivityChart, activity bars, words per day chart, 30-day chart, Recharts BarChart, chart tooltip
 * WHAT:  The activity series as a bar per local day (words, --color-chart-1) on a hairline baseline
 *        (--color-separator), date labels under it, and a glass tooltip with the day, its words and its takes.
 * WHY:   04 §5 "activity card (30-day bars)". Rust sends every day of the window, zeros included, so the bars
 *        line up with the calendar and an idle day is a visible gap on the baseline (no fake minimum bar; Recharts
 *        draws no background for a zero bar, so there is no track either). Colours are token references and geometry
 *        comes from the chart JS tokens (styles/placement.ts), so the chart follows every theme. Animation is
 *        off: data only changes after a take, and a regrowing chart would draw the eye for no reason (and needs
 *        no reduced-motion variant). The tooltip reads the hovered day from this component's own data by its
 *        date, never from Recharts' untyped payload. Recharts' accessibility layer lets the keyboard walk the bars.
 * WHERE: routes/dashboard/_components/ActivityCard.tsx (lazy-loaded: the default export is the component).
 */
import { useMemo } from "react";
import { Bar, BarChart, XAxis } from "recharts";
import type { ActivityDay } from "@/bindings";
import { ChartContainer, ChartTooltip, ChartTooltipContent, type ChartContainerProps } from "@/components/ui/Chart";
import { formatCount, formatDay } from "@/lib/format";
import { CHART_BAR_GAP, CHART_BAR_RADIUS, CHART_MIN_TICK_GAP, CHART_TICK_MARGIN } from "@/styles/placement";

const BAR_COLOR = "var(--color-chart-1)";
const BASELINE = { stroke: "var(--color-separator)" };
const CURSOR = { fill: "var(--color-fill-hover)" };
const MARGIN = { top: 0, bottom: 0, left: CHART_TICK_MARGIN, right: CHART_TICK_MARGIN };
/** Bars stand on the baseline, so only their top corners are rounded. */
const TOP_CORNERS: [number, number, number, number] = [CHART_BAR_RADIUS, CHART_BAR_RADIUS, 0, 0];

export interface ActivityChartProps {
  readonly days: readonly ActivityDay[];
  /** Accessible name of the chart. */
  readonly label: string;
  readonly initialDimension?: ChartContainerProps["initialDimension"];
}

export default function ActivityChart({ days, label, initialDimension }: ActivityChartProps) {
  const data = useMemo(() => [...days], [days]);
  const byDate = useMemo(() => new Map(days.map((day) => [day.date, day])), [days]);
  return (
    <ChartContainer {...(initialDimension === undefined ? {} : { initialDimension })}>
      <BarChart data={data} margin={MARGIN} barCategoryGap={CHART_BAR_GAP} title={label}>
        <XAxis
          dataKey="date"
          tickLine={false}
          axisLine={BASELINE}
          tickMargin={CHART_TICK_MARGIN}
          minTickGap={CHART_MIN_TICK_GAP}
          interval="preserveStartEnd"
          tickFormatter={(value: unknown) => (typeof value === "string" ? formatDay(value) : "")}
        />
        <ChartTooltip
          cursor={CURSOR}
          isAnimationActive={false}
          content={({ active, label: date }) => {
            const day = typeof date === "string" ? byDate.get(date) : undefined;
            return day === undefined ? null : (
              <ChartTooltipContent
                active={active}
                title={formatDay(day.date, "long")}
                rows={[
                  { key: "words", label: "Words", value: formatCount(day.words), color: BAR_COLOR },
                  { key: "takes", label: "Takes", value: formatCount(day.transcriptions) },
                ]}
              />
            );
          }}
        />
        <Bar
          dataKey="words"
          name="Words"
          fill={BAR_COLOR}
          radius={TOP_CORNERS}
          isAnimationActive={false}
        />
      </BarChart>
    </ChartContainer>
  );
}
