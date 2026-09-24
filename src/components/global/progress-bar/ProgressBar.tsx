/**
 * SOURCE OF TRUTH KEYWORDS: ProgressBar, progress, determinate progress, indeterminate progress, download progress, loading delay, Radix Progress
 * WHAT:  A thin token-styled progress bar. With a number `value` (0…`max`) it fills proportionally; with
 *        `value={null}` it is indeterminate: a segment sweeps across, revealed only after --delay-loading.
 * WHY:   04 §6 asks for one bar in both modes (model downloads report bytes/total, verification has no total).
 *        Radix Progress supplies the progressbar role and aria values. The fill is a GPU scale driven by one CSS
 *        variable instead of a width, so updates at 10 Hz (ModelProgress) never trigger layout. "Nothing waits"
 *        (04 §1): the indeterminate bar fades in after --delay-loading so fast operations never flash it; under
 *        reduced motion it shows a static full-width track instead of the sweep. A value outside 0…max is clamped.
 * WHERE: Models page cards and onboarding model step (step 21, 24). Exported through components/global/index.ts.
 */
import { Progress } from "radix-ui";
import type { ComponentProps, CSSProperties } from "react";
import { cn } from "@/lib/cn";
import { progressFraction } from "./progress-fraction";

export type ProgressBarProps = Omit<ComponentProps<typeof Progress.Root>, "value" | "max" | "children"> & {
  /** Progress so far, from 0 to `max`; `null` means the total is unknown (indeterminate). */
  readonly value: number | null;
  readonly max?: number;
};

const DEFAULT_MAX = 100;

type FillStyle = CSSProperties & Record<"--echo-progress", number>;

export function ProgressBar({ value, max = DEFAULT_MAX, className, ...props }: ProgressBarProps) {
  // Radix rejects a non-positive max; such a total is unknown, which the caller should pass as `value={null}`.
  const total = max > 0 && Number.isFinite(max) ? max : DEFAULT_MAX;
  const fraction = value === null ? null : progressFraction(value, total);
  const fill: FillStyle | undefined = fraction === null ? undefined : { "--echo-progress": fraction };
  return (
    <Progress.Root
      data-slot="progress-bar"
      value={fraction === null ? null : fraction * total}
      max={total}
      className={cn(
        "relative h-track w-full overflow-hidden rounded-pill bg-fill-pressed",
        "data-[state=indeterminate]:animate-fade-in data-[state=indeterminate]:[animation-delay:var(--delay-loading)]",
        className,
      )}
      {...props}
    >
      <Progress.Indicator
        data-slot="progress-bar-indicator"
        style={fill}
        className={cn(
          "h-full rounded-pill bg-accent",
          fraction === null
            ? "w-1/3 motion-safe:animate-indeterminate motion-reduce:w-full motion-reduce:bg-chart-2"
            : "w-full origin-left scale-x-(--echo-progress) transition-[scale] duration-(--duration-base) ease-standard",
        )}
      />
    </Progress.Root>
  );
}
