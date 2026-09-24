/**
 * SOURCE OF TRUTH KEYWORDS: Tooltip, TooltipProvider, TooltipTrigger, TooltipContent, hover hint, Radix Tooltip, popover glass
 * WHAT:  The shadcn Tooltip parts restyled to Echo tokens: the content is a `popover` GlassSurface with
 *        --text-footnote copy, at most --tooltip-max-width wide, fading (and on motion-safe scaling) in.
 * WHY:   Icon-only buttons (titlebar, row actions) need a name on hover and focus; Radix supplies the delay,
 *        keyboard focus behaviour and aria-describedby. The glass recipe comes from GlassSurface (04 §3.2), never
 *        restated here. Radix requires a provider, so each Tooltip brings one (the shadcn v4 shape) and a single
 *        tooltip works anywhere.
 * WHERE: App shell titlebar buttons (app/shell/Titlebar.tsx), History row actions (step 16). Exported through
 *        components/ui/index.ts.
 */
import { Tooltip as TooltipPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { GlassSurface } from "@/components/global/glass-surface";
import { cn } from "@/lib/cn";
import { POPOVER_SIDE_OFFSET } from "@/styles/placement";

export const TooltipProvider = TooltipPrimitive.Provider;
export const TooltipTrigger = TooltipPrimitive.Trigger;

/** One tooltip; carries its own provider (Radix requires one) so it works anywhere. */
export function Tooltip(props: ComponentProps<typeof TooltipPrimitive.Root>) {
  return (
    <TooltipProvider>
      <TooltipPrimitive.Root data-slot="tooltip" {...props} />
    </TooltipProvider>
  );
}

export function TooltipContent({
  className,
  sideOffset = POPOVER_SIDE_OFFSET,
  children,
  ...props
}: ComponentProps<typeof TooltipPrimitive.Content>) {
  return (
    <TooltipPrimitive.Portal>
      <GlassSurface variant="popover" asChild>
        <TooltipPrimitive.Content
          data-slot="tooltip-content"
          sideOffset={sideOffset}
          className={cn(
            "z-(--z-popover) max-w-tooltip px-2 py-1 text-footnote text-fg",
            "animate-fade-in motion-safe:animate-scale-in data-[state=closed]:animate-fade-out",
            "origin-(--radix-tooltip-content-transform-origin)",
            className,
          )}
          {...props}
        >
          {children}
        </TooltipPrimitive.Content>
      </GlassSurface>
    </TooltipPrimitive.Portal>
  );
}
