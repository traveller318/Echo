/**
 * SOURCE OF TRUTH KEYWORDS: Slider, range input, Int setting control, Radix Slider, track, thumb
 * WHAT:  The shadcn Slider restyled to Echo tokens: a --track-height track in --color-fill-pressed, the filled
 *        range in --color-accent, and one --slider-thumb knob per value.
 * WHY:   Bounded Int settings (cancel countdown, retention days, typing WPM) render as a slider (step 18); Radix
 *        supplies keyboard steps, aria values and pointer capture. One thumb is rendered per value so a range
 *        slider needs no second component. `aria-label` is forwarded to the thumbs, because Radix puts the slider role
 *        on each thumb and a label on the root span would name nothing.
 * WHERE: SettingField for bounded Int settings. Exported through components/ui/index.ts.
 */
import { Slider as SliderPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";

export function Slider({
  className,
  value,
  defaultValue,
  "aria-label": ariaLabel,
  ...props
}: ComponentProps<typeof SliderPrimitive.Root>) {
  // With neither prop Radix starts at one value (`min`), so one thumb.
  const thumbCount = (value ?? defaultValue)?.length ?? 1;
  return (
    <SliderPrimitive.Root
      data-slot="slider"
      value={value}
      defaultValue={defaultValue}
      className={cn(
        "relative flex w-full touch-none items-center select-none",
        "data-[disabled]:pointer-events-none data-[disabled]:opacity-(--opacity-disabled)",
        className,
      )}
      {...props}
    >
      <SliderPrimitive.Track
        data-slot="slider-track"
        className="relative h-track w-full grow overflow-hidden rounded-pill bg-fill-pressed"
      >
        <SliderPrimitive.Range data-slot="slider-range" className="absolute h-full bg-accent" />
      </SliderPrimitive.Track>
      {Array.from({ length: thumbCount }, (_, index) => (
        <SliderPrimitive.Thumb
          // Thumbs are positional: Radix pairs the nth thumb with the nth value.
          key={index}
          // The thumb carries role="slider", so the accessible name belongs on it, not on the root span.
          aria-label={ariaLabel}
          data-slot="slider-thumb"
          className={cn(
            "block size-slider-thumb rounded-pill border-(length:--border-hairline) border-separator bg-accent-fg shadow-e1",
            "transition-[scale] duration-(--duration-fast) ease-standard motion-safe:active:scale-(--press-scale)",
          )}
        />
      ))}
    </SliderPrimitive.Root>
  );
}
