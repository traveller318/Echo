/**
 * SOURCE OF TRUTH KEYWORDS: Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription, SheetFooter, side drawer, detail drawer, Radix Dialog
 * WHAT:  The shadcn Sheet (a Radix Dialog shaped as a side drawer) restyled to Echo tokens: a --sheet-width
 *        `modal` GlassSurface sliding in from the right edge over the shared DialogOverlay.
 * WHY:   History shows a take's full text in a detail drawer (04 §5) without leaving the list. It reuses the
 *        Dialog overlay and corner close button instead of restyling them (root CLAUDE.md §7), and the glass
 *        recipe comes from GlassSurface; only the drawer's own edge radius is overridden. The slide runs under
 *        motion-safe; reduced motion gets the fade.
 * WHERE: History detail drawer (step 16). Exported through components/ui/index.ts.
 */
import { Dialog as DialogPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { GlassSurface } from "@/components/global/glass-surface";
import { cn } from "@/lib/cn";
import { DialogCornerClose, DialogOverlay } from "./Dialog";

export const Sheet = DialogPrimitive.Root;
export const SheetTrigger = DialogPrimitive.Trigger;
export const SheetClose = DialogPrimitive.Close;

export type SheetContentProps = ComponentProps<typeof DialogPrimitive.Content> & {
  /** Show the corner close button (default). */
  readonly showClose?: boolean;
};

export function SheetContent({ className, children, showClose = true, ...props }: SheetContentProps) {
  return (
    <DialogPrimitive.Portal>
      <DialogOverlay />
      {/* The override goes through GlassSurface so cn resolves it against the variant radius. */}
      <GlassSurface variant="modal" asChild className="rounded-none rounded-l-card">
        <DialogPrimitive.Content
          data-slot="sheet-content"
          className={cn(
            "fixed inset-y-0 right-0 z-(--z-modal) flex h-full w-full max-w-sheet flex-col gap-4 p-6",
            "animate-fade-in motion-safe:animate-slide-in-right",
            "data-[state=closed]:animate-fade-out motion-safe:data-[state=closed]:animate-slide-out-right",
            className,
          )}
          {...props}
        >
          {children}
          {showClose ? <DialogCornerClose /> : null}
        </DialogPrimitive.Content>
      </GlassSurface>
    </DialogPrimitive.Portal>
  );
}

export function SheetHeader({ className, ...props }: ComponentProps<"div">) {
  return <div data-slot="sheet-header" className={cn("flex flex-col gap-1 pr-8", className)} {...props} />;
}

export function SheetFooter({ className, ...props }: ComponentProps<"div">) {
  return <div data-slot="sheet-footer" className={cn("mt-auto flex justify-end gap-2", className)} {...props} />;
}

export function SheetTitle({ className, ...props }: ComponentProps<typeof DialogPrimitive.Title>) {
  return <DialogPrimitive.Title data-slot="sheet-title" className={cn("text-title3 text-fg", className)} {...props} />;
}

export function SheetDescription({ className, ...props }: ComponentProps<typeof DialogPrimitive.Description>) {
  return (
    <DialogPrimitive.Description
      data-slot="sheet-description"
      className={cn("text-body text-fg-secondary", className)}
      {...props}
    />
  );
}
