/**
 * SOURCE OF TRUTH KEYWORDS: toast store, useToastStore, showToast, showAppErrorToast, dismissToast, ToastEntry, ToastEntryAction, zustand, in-app notifications
 * WHAT:  The in-app toast queue (Zustand, UI state only): `showToast(input)` adds a toast, `showAppErrorToast(error)`
 *        adds the calm copy of an AppError with its action, `dismissToast(id)` closes one. At most TOAST_LIMIT stay
 *        open; the oldest open toast closes when a newer one would exceed it.
 * WHY:   Anything can report something (a failed mutation, a window control, a query error) without a React
 *        context, so the queue lives outside the tree and the Toaster (app/shell) renders it. A toast holds data,
 *        not behaviour for errors: an AppError action is kept as its id and resolved by the shell (navigation or
 *        a command), so this store never imports the router or the bindings. A closed toast stays in the list,
 *        closed, so Radix can play its exit animation; closed entries are pruned the next time one is shown.
 *        The copy comes from lib/app-error.ts, the single error copy table.
 * WHERE: Rendered by app/shell/Toaster.tsx; called by hooks/use-echo-mutation.ts, the titlebar and the shell's
 *        error actions; later by History ("Copied") and recovery notices.
 */
import { create } from "zustand";
import type { AppError } from "@/bindings";
import { describeAppError, type AppErrorAction } from "@/lib/app-error";
import { TOAST_LIMIT } from "@/styles/placement";

/** What a toast's one button does. */
export type ToastEntryAction =
  /** An AppError action; the shell maps its id to a page or a command. */
  | { readonly type: "app-error"; readonly action: AppErrorAction }
  /** Anything else, e.g. "Undo"; `altText` says how to do it without the toast (screen readers). */
  | { readonly type: "callback"; readonly label: string; readonly altText: string; readonly run: () => void };

/** How the toast is marked; status is never colour alone (04 §7), so `error` also adds a glyph. */
export type ToastTone = "neutral" | "error";

export interface ToastInput {
  readonly title: string;
  readonly body?: string;
  readonly tone?: ToastTone;
  readonly action?: ToastEntryAction;
}

export interface ToastEntry {
  readonly id: number;
  readonly title: string;
  readonly body: string | null;
  readonly tone: ToastTone;
  readonly action: ToastEntryAction | null;
  readonly open: boolean;
}

interface ToastState {
  readonly toasts: readonly ToastEntry[];
  readonly nextId: number;
}

export const useToastStore = create<ToastState>()(() => ({ toasts: [], nextId: 1 }));

/** Shows a toast and returns its id. */
export function showToast(input: ToastInput): number {
  const { nextId } = useToastStore.getState();
  const entry: ToastEntry = {
    id: nextId,
    title: input.title,
    body: input.body ?? null,
    tone: input.tone ?? "neutral",
    action: input.action ?? null,
    open: true,
  };
  useToastStore.setState((state) => {
    const open = [...state.toasts.filter((toast) => toast.open), entry];
    const overflow = Math.max(0, open.length - TOAST_LIMIT);
    return {
      nextId: state.nextId + 1,
      toasts: open.map((toast, index) => (index < overflow ? { ...toast, open: false } : toast)),
    };
  });
  return nextId;
}

/** Shows the calm copy of an AppError (lib/app-error.ts) with its action, if it has one. */
export function showAppErrorToast(error: AppError): number {
  const copy = describeAppError(error);
  return showToast({
    title: copy.title,
    body: copy.body,
    tone: "error",
    ...(copy.action === null ? {} : { action: { type: "app-error", action: copy.action } }),
  });
}

/** Closes a toast; it leaves with its exit animation and is pruned later. */
export function dismissToast(id: number): void {
  useToastStore.setState((state) => ({
    toasts: state.toasts.map((toast) => (toast.id === id ? { ...toast, open: false } : toast)),
  }));
}
