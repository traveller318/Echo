/**
 * SOURCE OF TRUTH KEYWORDS: RadioGroup, RadioGroupItem, Radix RadioGroup, single choice, segmented control, choice cards, arrow key choice
 * WHAT:  The shadcn RadioGroup reduced to Echo tokens: a Root that lays its items out and an Item that is a pressable
 *        choice with `data-state="checked"` when selected. It draws no dot: callers style an item as a segment of a
 *        segmented control or as a card.
 * WHY:   A choice among a few options drawn as buttons or pictures still needs radio semantics (one tab stop, arrow
 *        keys move the choice, aria-checked); Radix supplies them, so segmented rows and preview cards never
 *        re-implement keyboard handling. The visual shape lives with the caller because the two shapes share no
 *        layout; this file only owns the interaction states every item has (hover, press, disabled).
 * WHERE: components/global/setting-field (EnumControl's segmented and cards displays). Exported through
 *        components/ui/index.ts.
 */
import { RadioGroup as RadioGroupPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";

export function RadioGroup({ className, ...props }: ComponentProps<typeof RadioGroupPrimitive.Root>) {
  return <RadioGroupPrimitive.Root data-slot="radio-group" className={cn("flex", className)} {...props} />;
}

export function RadioGroupItem({ className, ...props }: ComponentProps<typeof RadioGroupPrimitive.Item>) {
  return (
    <RadioGroupPrimitive.Item
      data-slot="radio-group-item"
      className={cn(
        "select-none transition-[background-color,color,border-color] duration-(--duration-fast) ease-standard",
        "disabled:pointer-events-none disabled:opacity-(--opacity-disabled)",
        className,
      )}
      {...props}
    />
  );
}
