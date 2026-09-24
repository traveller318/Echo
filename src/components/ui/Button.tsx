/**
 * SOURCE OF TRUTH KEYWORDS: Button, buttonVariants, primary button, secondary button, ghost button, destructive button, icon button, asChild
 * WHAT:  The shadcn Button restyled to Echo tokens: variants primary / secondary / ghost / destructive, sizes
 *        sm / md / icon-sm / icon-md, and `asChild` to render the styles on a child (a router link).
 * WHY:   04 §6 fixes the variant and size set. Heights are --size-hit (28px, the minimum hit target, 04 §7) and
 *        --size-control; text is --text-callout. Press feedback is a CSS scale to --press-scale only under
 *        motion-safe, so reduced motion keeps a colour change only. Disabled uses --opacity-disabled. Icons inside
 *        get --size-icon-sm unless they set their own size.
 * WHERE: Everywhere a command is triggered (dialogs, empty-state actions, settings, models). Exported through
 *        components/ui/index.ts.
 */
import type { VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";
import { buttonVariants } from "./button-variants";

export type ButtonProps = ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    /** Render the button styles on the single child instead of a <button>. */
    readonly asChild?: boolean;
  };

export function Button({ className, variant, size, asChild = false, type, ...props }: ButtonProps) {
  const Component = asChild ? Slot.Root : "button";
  return (
    <Component
      data-slot="button"
      // A plain <button> defaults to "submit"; inside a form that would submit on every click.
      type={asChild ? type : (type ?? "button")}
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  );
}
