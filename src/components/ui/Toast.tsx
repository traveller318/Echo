/**
 * SOURCE OF TRUTH KEYWORDS: Toast, ToastProvider, ToastViewport, ToastTitle, ToastDescription, ToastAction, ToastClose, in-app notification, Radix Toast
 * WHAT:  The toast parts restyled to Echo tokens: a provider (TOAST_DURATION_MS, swipe right to dismiss), a
 *        bottom-right viewport at most --toast-width wide above everything but modals (--z-toast), and a
 *        `popover` GlassSurface toast with title, description, one action button and a dismiss button.
 * WHY:   Built on Radix Toast instead of Sonner (the current shadcn default) because Sonner injects an inline
 *        <style> element that the production CSP blocks (05 §5); Radix Toast styles through classes only and
 *        brings the live region, pause-on-hover/focus, F8 hotkey to the viewport and swipe gestures. The action
 *        reuses Button and the dismiss control reuses the ghost icon Button (root CLAUDE.md §7). Copy is calm and
 *        short (04 §1); the viewport is announced as "Echo notifications". The imperative toaster (a queue that
 *        renders these parts) lands with the app providers in step 08.
 * WHERE: The toaster in src/app/providers.tsx (step 08): AppError toasts via lib/app-error.ts, "Copied" and
 *        recovery notices. Exported through components/ui/index.ts.
 */
import { XIcon } from "lucide-react";
import { Toast as ToastPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { GlassSurface } from "@/components/global/glass-surface";
import { cn } from "@/lib/cn";
import { TOAST_DURATION_MS } from "@/styles/placement";
import { Button } from "./Button";

export function ToastProvider({
  duration = TOAST_DURATION_MS,
  swipeDirection = "right",
  label = "Echo notifications",
  ...props
}: ComponentProps<typeof ToastPrimitive.Provider>) {
  return <ToastPrimitive.Provider duration={duration} swipeDirection={swipeDirection} label={label} {...props} />;
}

export function ToastViewport({ className, ...props }: ComponentProps<typeof ToastPrimitive.Viewport>) {
  return (
    <ToastPrimitive.Viewport
      data-slot="toast-viewport"
      className={cn(
        "fixed right-0 bottom-0 z-(--z-toast) m-0 flex max-h-full w-full max-w-toast list-none flex-col gap-2 p-4 outline-none",
        className,
      )}
      {...props}
    />
  );
}

export function Toast({ className, ...props }: ComponentProps<typeof ToastPrimitive.Root>) {
  return (
    <GlassSurface variant="popover" asChild>
      <ToastPrimitive.Root
        data-slot="toast"
        className={cn(
          "pointer-events-auto relative flex w-full items-start gap-3 p-4 pr-10",
          "animate-fade-in motion-safe:animate-slide-in-up data-[state=closed]:animate-fade-out",
          "data-[swipe=move]:translate-x-(--radix-toast-swipe-move-x)",
          "data-[swipe=cancel]:translate-x-0 data-[swipe=cancel]:transition-[translate] data-[swipe=cancel]:duration-(--duration-base)",
          "data-[swipe=end]:translate-x-(--radix-toast-swipe-end-x) data-[swipe=end]:animate-slide-out-right",
          className,
        )}
        {...props}
      />
    </GlassSurface>
  );
}

export function ToastTitle({ className, ...props }: ComponentProps<typeof ToastPrimitive.Title>) {
  return <ToastPrimitive.Title data-slot="toast-title" className={cn("text-callout text-fg", className)} {...props} />;
}

export function ToastDescription({ className, ...props }: ComponentProps<typeof ToastPrimitive.Description>) {
  return (
    <ToastPrimitive.Description
      data-slot="toast-description"
      className={cn("text-footnote text-fg-secondary", className)}
      {...props}
    />
  );
}

export type ToastActionProps = ComponentProps<typeof ToastPrimitive.Action>;

/** The toast's one action; `altText` tells screen-reader users how to do it another way (Radix requires it). */
export function ToastAction({ className, children, ...props }: ToastActionProps) {
  return (
    <ToastPrimitive.Action asChild {...props}>
      <Button data-slot="toast-action" variant="secondary" size="sm" className={cn("shrink-0", className)}>
        {children}
      </Button>
    </ToastPrimitive.Action>
  );
}

export function ToastClose({ className, ...props }: ComponentProps<typeof ToastPrimitive.Close>) {
  return (
    <ToastPrimitive.Close asChild {...props}>
      <Button
        data-slot="toast-close"
        variant="ghost"
        size="icon-sm"
        aria-label="Dismiss"
        className={cn("absolute top-2 right-2", className)}
      >
        <XIcon aria-hidden="true" />
      </Button>
    </ToastPrimitive.Close>
  );
}
