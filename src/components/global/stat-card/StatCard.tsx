/**
 * SOURCE OF TRUTH KEYWORDS: StatCard, stat card, hero stat, metric card, label slot, value slot, unit slot, trend slot, footer slot, StatUnit
 * WHAT:  A glass card that leads with one number: `label` above, `value` large (with an optional `unit` after it
 *        and a `trend` at the end of its line), an optional `footer` below, and an optional `help` sentence behind
 *        an info button. Size `hero` sets the value in --text-display, `default` in --text-title1. `StatUnit`
 *        marks a unit word inside `value` (e.g. the `h` and `min` of a duration) so it takes the smaller unit style.
 * WHY:   04 §1 "numbers are heroes": figures are large and tabular, labels and unit words stay quiet. Every slot
 *        is a ReactNode, so the card carries no metric knowledge and a new metric or screen reuses it unchanged
 *        (04 §6). The card is a labelled group, so a screen reader announces the label with the number. `help` is a
 *        Tooltip on a real button, so it reaches keyboard users (04 §7), and it is also the group's description.
 *        Unit words take their size from the card's size through one data-slot rule instead of a prop per span,
 *        and are set off by real spaces, so the value reads (and is announced) exactly as the text elsewhere.
 * WHERE: routes/dashboard (hero, stat cards, small cards); later any screen that shows a headline number.
 *        Exported through components/global/index.ts.
 */
import { InfoIcon } from "lucide-react";
import { useId, type ComponentProps, type ReactNode } from "react";
import { Button, Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui";
import { cn } from "@/lib/cn";
import { NUMERIC_CLASS } from "@/lib/format";
import { GlassSurface } from "../glass-surface";

export type StatCardSize = "hero" | "default";

export type StatCardProps = Omit<ComponentProps<"div">, "title"> & {
  readonly label: ReactNode;
  readonly value: ReactNode;
  /** A unit word after the value (`wpm`); for units inside the value use StatUnit. */
  readonly unit?: ReactNode;
  /** A change indicator at the end of the value's line. */
  readonly trend?: ReactNode;
  /** A line under the value. */
  readonly footer?: ReactNode;
  /** One sentence on what the number means, behind an info button. */
  readonly help?: string;
  readonly size?: StatCardSize;
};

const VALUE_SIZE: Readonly<Record<StatCardSize, string>> = {
  hero: "text-display [&_[data-slot=stat-unit]]:text-title3",
  default: "text-title1 [&_[data-slot=stat-unit]]:text-callout",
};

const PADDING: Readonly<Record<StatCardSize, string>> = {
  hero: "p-6",
  default: "p-5",
};

export function StatCard({
  label,
  value,
  unit,
  trend,
  footer,
  help,
  size = "default",
  className,
  ...props
}: StatCardProps) {
  const labelId = useId();
  const helpId = useId();
  return (
    <GlassSurface
      role="group"
      aria-labelledby={labelId}
      {...(help === undefined ? {} : { "aria-describedby": helpId })}
      data-slot="stat-card"
      data-size={size}
      className={cn("flex min-w-0 flex-col gap-2", PADDING[size], className)}
      {...props}
    >
      <div data-slot="stat-card-header" className="flex min-h-hit items-center gap-1">
        <p id={labelId} className="min-w-0 truncate text-callout text-fg-secondary">
          {label}
        </p>
        {help === undefined ? null : (
          <>
            <span id={helpId} className="sr-only">
              {help}
            </span>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={typeof label === "string" ? `About ${label}` : "About this number"}
                  className="text-fg-tertiary"
                >
                  <InfoIcon aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>{help}</TooltipContent>
            </Tooltip>
          </>
        )}
      </div>
      <div data-slot="stat-card-value-row" className="flex min-w-0 flex-wrap items-baseline justify-between gap-x-3">
        <p data-slot="stat-card-value" className={cn("min-w-0 text-fg", NUMERIC_CLASS, VALUE_SIZE[size])}>
          {value}
          {unit === undefined ? null : (
            <>
              {" "}
              <StatUnit>{unit}</StatUnit>
            </>
          )}
        </p>
        {trend === undefined ? null : (
          <div data-slot="stat-card-trend" className="text-footnote text-fg-secondary">
            {trend}
          </div>
        )}
      </div>
      {footer === undefined ? null : (
        <div data-slot="stat-card-footer" className="text-footnote text-fg-secondary">
          {footer}
        </div>
      )}
    </GlassSurface>
  );
}

/** A unit word inside a StatCard value, sized by the card (smaller than the figures); put a space before it. */
export function StatUnit({ className, children, ...props }: ComponentProps<"span">) {
  return (
    <span data-slot="stat-unit" className={cn("text-fg-secondary", className)} {...props}>
      {children}
    </span>
  );
}
