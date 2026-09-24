/**
 * SOURCE OF TRUTH KEYWORDS: GlassSurface, glass recipe, glass variants, card, sidebar, popover, pill, modal, backdrop blur, frosted surface
 * WHAT:  A frosted surface: tint, backdrop blur + saturate, hairline border, inner top highlight and elevation,
 *        in one of five variants (card, sidebar, popover, pill, modal). Renders a div, or with `asChild` puts the
 *        recipe on its single child (e.g. a Radix Content part).
 * WHY:   docs/04 §3.2 makes this component (with its classes in glass-surface-variants.ts) the only place the
 *        glass recipe is assembled, so every glass surface in both windows changes together. Overrides go through
 *        `className` and cn, so they resolve against the variant's classes even with `asChild`.
 *        `data-variant` exposes the variant to tests and to styling hooks.
 * WHERE: Main-window cards and sidebar (step 08+), the pill (step 15), and the popover/modal parts of
 *        components/ui (Select, Tooltip, Toast, Dialog, Sheet). Exported through components/global/index.ts.
 */
import { Slot } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";
import { glassSurfaceVariants, type GlassSurfaceVariant } from "./glass-surface-variants";

export type GlassSurfaceProps = ComponentProps<"div"> & {
  readonly variant?: GlassSurfaceVariant;
  /** Put the recipe on the single child instead of rendering a div. */
  readonly asChild?: boolean;
};

export function GlassSurface({ variant = "card", asChild = false, className, ...props }: GlassSurfaceProps) {
  const Component = asChild ? Slot.Root : "div";
  return (
    <Component
      data-slot="glass-surface"
      data-variant={variant}
      className={cn(glassSurfaceVariants({ variant }), className)}
      {...props}
    />
  );
}
