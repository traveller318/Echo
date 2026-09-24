/**
 * SOURCE OF TRUTH KEYWORDS: Dialog, DialogContent, DialogHeader, DialogFooter, DialogTitle, DialogDescription, DialogClose, DialogOverlay, modal, confirm dialog, Radix Dialog
 * WHAT:  The shadcn Dialog parts restyled to Echo tokens: a --color-scrim overlay and a centered `modal`
 *        GlassSurface at most --dialog-width wide, with header, footer, title, description and a close button.
 * WHY:   Confirmations (delete a take, remove a model) and short forms need a focus-trapped modal; Radix supplies
 *        the focus trap, Esc/outside-click dismissal, aria-modal and scroll lock. The scroll lock injects a
 *        <style> tag, which passes the production CSP only because lib/csp-nonce.ts hands it the page nonce.
 *        The close button's name is "Close" (calm copy, 04 §1). Scale-in runs only under motion-safe; reduced
 *        motion keeps the fade.
 * WHERE: History delete confirmation (step 16), Models remove confirmation (step 21). Sheet (Sheet.tsx) shares
 *        DialogOverlay. Exported through components/ui/index.ts.
 */
import { XIcon } from "lucide-react";
import { Dialog as DialogPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { GlassSurface } from "@/components/global/glass-surface";
import { cn } from "@/lib/cn";
import { Button } from "./Button";

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogPortal = DialogPrimitive.Portal;
export const DialogClose = DialogPrimitive.Close;

export function DialogOverlay({ className, ...props }: ComponentProps<typeof DialogPrimitive.Overlay>) {
  return (
    <DialogPrimitive.Overlay
      data-slot="dialog-overlay"
      className={cn(
        "fixed inset-0 z-(--z-modal) bg-scrim",
        "animate-fade-in data-[state=closed]:animate-fade-out",
        className,
      )}
      {...props}
    />
  );
}

/** A close button in the top-right corner of a modal surface. */
export function DialogCornerClose({ className, ...props }: ComponentProps<typeof DialogPrimitive.Close>) {
  return (
    <DialogPrimitive.Close asChild {...props}>
      <Button variant="ghost" size="icon-sm" aria-label="Close" className={cn("absolute top-3 right-3", className)}>
        <XIcon aria-hidden="true" />
      </Button>
    </DialogPrimitive.Close>
  );
}

export type DialogContentProps = ComponentProps<typeof DialogPrimitive.Content> & {
  /** Show the corner close button (default). Turn off when the footer holds the only way out. */
  readonly showClose?: boolean;
};

export function DialogContent({ className, children, showClose = true, ...props }: DialogContentProps) {
  return (
    <DialogPortal>
      <DialogOverlay />
      <GlassSurface variant="modal" asChild>
        <DialogPrimitive.Content
          data-slot="dialog-content"
          className={cn(
            "fixed top-1/2 left-1/2 z-(--z-modal) grid w-full max-w-dialog -translate-x-1/2 -translate-y-1/2 gap-4 p-6",
            "animate-fade-in motion-safe:animate-scale-in data-[state=closed]:animate-fade-out",
            className,
          )}
          {...props}
        >
          {children}
          {showClose ? <DialogCornerClose /> : null}
        </DialogPrimitive.Content>
      </GlassSurface>
    </DialogPortal>
  );
}

export function DialogHeader({ className, ...props }: ComponentProps<"div">) {
  return <div data-slot="dialog-header" className={cn("flex flex-col gap-1 pr-8", className)} {...props} />;
}

export function DialogFooter({ className, ...props }: ComponentProps<"div">) {
  return <div data-slot="dialog-footer" className={cn("flex justify-end gap-2", className)} {...props} />;
}

export function DialogTitle({ className, ...props }: ComponentProps<typeof DialogPrimitive.Title>) {
  return <DialogPrimitive.Title data-slot="dialog-title" className={cn("text-title3 text-fg", className)} {...props} />;
}

export function DialogDescription({ className, ...props }: ComponentProps<typeof DialogPrimitive.Description>) {
  return (
    <DialogPrimitive.Description
      data-slot="dialog-description"
      className={cn("text-body text-fg-secondary", className)}
      {...props}
    />
  );
}
