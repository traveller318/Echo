/**
 * SOURCE OF TRUTH KEYWORDS: Chart, ChartContainer, ChartTooltip, ChartTooltipContent, ChartTooltipRow, shadcn charts, Recharts, chart tokens, --color-chart
 * WHAT:  The shadcn chart parts restyled to Echo tokens: ChartContainer (a --chart-height, full-width responsive
 *        box that styles the Recharts parts inside it with tokens), ChartTooltip (Recharts' Tooltip) and
 *        ChartTooltipContent (a `popover` GlassSurface with a heading and one row per series: swatch, label,
 *        tabular value).
 * WHY:   02 §2.4 picks shadcn charts (Recharts) for the dashboard. shadcn's ChartStyle injects a <style> element
 *        per chart for its colour variables, which the production CSP (`style-src 'self'`, 02 §10) blocks; here
 *        series colours are plain token references (`var(--color-chart-1)`) passed to Recharts and axis text is styled
 *        through classes on the container, so nothing is injected (Recharts itself only sets CSSOM styles, which
 *        the CSP allows). Recharts' own text colours are presentation attributes, which these classes override,
 *        so charts follow light, dark and reduced-transparency themes with no JS. The tooltip content is
 *        data-agnostic: the chart looks its datum up and passes rows, so no untyped Recharts payload is read.
 *        `initialDimension` is the size assumed before the container is measured (tests, where jsdom has no
 *        ResizeObserver).
 * WHERE: routes/dashboard ActivityChart. Imported as `@/components/ui/Chart`, never through the ui barrel, so
 *        Recharts stays in the lazily loaded chunk of the page that draws a chart (see components/ui/index.ts).
 */
import type { ComponentProps, ReactElement, ReactNode } from "react";
import { ResponsiveContainer, Tooltip as RechartsTooltip } from "recharts";
import { GlassSurface } from "@/components/global/glass-surface";
import { cn } from "@/lib/cn";
import { NUMERIC_CLASS } from "@/lib/format";

/** Recharts' Tooltip; give it `content` rendering ChartTooltipContent. */
export const ChartTooltip = RechartsTooltip;

export type ChartContainerProps = Omit<ComponentProps<"div">, "children"> & {
  /** One Recharts chart (BarChart, AreaChart, …). */
  readonly children: ReactElement;
  /** Size assumed before the container is measured. */
  readonly initialDimension?: { readonly width: number; readonly height: number };
};

export function ChartContainer({ children, initialDimension, className, ...props }: ChartContainerProps) {
  return (
    <div
      data-slot="chart"
      className={cn(
        "h-chart w-full min-w-0 text-caption",
        "[&_.recharts-cartesian-axis-tick_text]:fill-fg-secondary [&_.recharts-text]:fill-fg-secondary",
        "[&_.recharts-surface]:overflow-visible",
        className,
      )}
      {...props}
    >
      <ResponsiveContainer
        {...(initialDimension === undefined
          ? {}
          : { initialDimension: { width: initialDimension.width, height: initialDimension.height } })}
      >
        {children}
      </ResponsiveContainer>
    </div>
  );
}

/** One line of a chart tooltip. */
export interface ChartTooltipRow {
  readonly key: string;
  readonly label: ReactNode;
  readonly value: ReactNode;
  /** The series colour as a token reference, e.g. `var(--color-chart-1)`; omit for a row without a swatch. */
  readonly color?: string;
}

export interface ChartTooltipContentProps {
  /** Recharts' `active`: the pointer or keyboard is on a datum. */
  readonly active?: boolean;
  readonly title?: ReactNode;
  readonly rows: readonly ChartTooltipRow[];
  readonly className?: string;
}

export function ChartTooltipContent({ active = false, title, rows, className }: ChartTooltipContentProps) {
  if (!active || rows.length === 0) {
    return null;
  }
  return (
    <GlassSurface
      variant="popover"
      data-slot="chart-tooltip"
      className={cn("flex min-w-0 flex-col gap-1 px-3 py-2 text-footnote text-fg", className)}
    >
      {title === undefined ? null : <p className="text-fg">{title}</p>}
      {rows.map((row) => (
        <div key={row.key} className="flex items-center gap-2">
          {row.color === undefined ? null : (
            <span
              aria-hidden="true"
              data-slot="chart-tooltip-swatch"
              className="size-2 shrink-0 rounded-xs"
              style={{ backgroundColor: row.color }}
            />
          )}
          <span className="text-fg-secondary">{row.label}</span>
          <span className={cn("ms-auto ps-3 text-fg", NUMERIC_CLASS)}>{row.value}</span>
        </div>
      ))}
    </GlassSurface>
  );
}
