/**
 * SOURCE OF TRUTH KEYWORDS: Toaster, toast queue renderer, toast viewport, toast action, error toast glyph, useToastStore
 * WHAT:  Renders the toast store (stores/toast-store.ts) with the Toast parts: title, optional body, an error glyph
 *        for `error` toasts, the one action button and a dismiss button, plus the viewport they stack in.
 * WHY:   The store holds data and this component turns it into Radix toasts, which bring timing, pause on hover or
 *        focus, swipe and the live region. AppError actions are resolved here with useAppErrorAction, because only
 *        the routed shell can navigate or knows the registry routes. A closed toast stays mounted with `open`
 *        false so Radix plays its exit animation. Error status is a glyph plus copy, never colour alone (04 §7).
 * WHERE: app/shell/ShellLayout.tsx, inside the ToastProvider from app/providers.tsx.
 */
import { TriangleAlertIcon } from "lucide-react";
import { Toast, ToastAction, ToastClose, ToastDescription, ToastTitle, ToastViewport } from "@/components/ui";
import { dismissToast, useToastStore, type ToastEntryAction } from "@/stores/toast-store";
import { useAppErrorAction } from "./use-app-error-action";

export function Toaster() {
  const toasts = useToastStore((state) => state.toasts);
  const runAppErrorAction = useAppErrorAction();

  const actionButton = (action: ToastEntryAction) =>
    action.type === "app-error" ? (
      <ToastAction
        altText={action.action.label}
        onClick={() => {
          runAppErrorAction(action.action.id);
        }}
      >
        {action.action.label}
      </ToastAction>
    ) : (
      <ToastAction altText={action.altText} onClick={action.run}>
        {action.label}
      </ToastAction>
    );

  return (
    <>
      {toasts.map((toast) => (
        <Toast
          key={toast.id}
          open={toast.open}
          onOpenChange={(open) => {
            if (!open) {
              dismissToast(toast.id);
            }
          }}
        >
          {toast.tone === "error" ? (
            <TriangleAlertIcon aria-hidden="true" className="size-icon-sm shrink-0 text-warning" />
          ) : null}
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <ToastTitle>{toast.title}</ToastTitle>
            {toast.body === null ? null : <ToastDescription>{toast.body}</ToastDescription>}
          </div>
          {toast.action === null ? null : actionButton(toast.action)}
          <ToastClose />
        </Toast>
      ))}
      <ToastViewport />
    </>
  );
}
