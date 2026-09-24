/**
 * SOURCE OF TRUTH KEYWORDS: toast store test, showToast test, showAppErrorToast test, TOAST_LIMIT test, dismissToast test
 * WHAT:  Verifies toasts are added open with defaults, AppErrors become their calm copy with the action id, the
 *        oldest open toast closes beyond TOAST_LIMIT, dismissing only closes, and closed toasts are pruned later.
 * WHY:   Every failure in the UI reaches the user through this queue; a toast that never closes or loses its
 *        action would leave the user without a way forward.
 * WHERE: Runs in the `web` Vitest project.
 */
import { beforeEach, describe, expect, it } from "vitest";
import { describeAppError } from "@/lib/app-error";
import { TOAST_LIMIT } from "@/styles/placement";
import { dismissToast, showAppErrorToast, showToast, useToastStore } from "./toast-store";

beforeEach(() => {
  useToastStore.setState({ toasts: [], nextId: 1 });
});

describe("toast store", () => {
  it("adds an open neutral toast with no body or action by default", () => {
    const id = showToast({ title: "Copied" });
    expect(useToastStore.getState().toasts).toEqual([
      { id, title: "Copied", body: null, tone: "neutral", action: null, open: true },
    ]);
  });

  it("shows an AppError as its copy with the action id", () => {
    showAppErrorToast({ code: "ModelMissing", model_id: "parakeet-tdt-0.6b-v3" });
    const copy = describeAppError({ code: "ModelMissing", model_id: "parakeet-tdt-0.6b-v3" });
    const [toast] = useToastStore.getState().toasts;
    expect(toast).toMatchObject({ title: copy.title, body: copy.body, tone: "error" });
    expect(toast?.action).toEqual({ type: "app-error", action: copy.action });
  });

  it("shows an AppError without an action as a toast without a button", () => {
    showAppErrorToast({ code: "Busy" });
    expect(useToastStore.getState().toasts[0]?.action).toBeNull();
  });

  it("closes the oldest open toast beyond the limit and prunes closed ones on the next show", () => {
    const ids = Array.from({ length: TOAST_LIMIT + 1 }, (_, index) => showToast({ title: `Toast ${String(index)}` }));
    const open = useToastStore.getState().toasts.filter((toast) => toast.open);
    expect(open.map((toast) => toast.id)).toEqual(ids.slice(1));
    expect(useToastStore.getState().toasts.find((toast) => toast.id === ids[0])?.open).toBe(false);

    showToast({ title: "Later" });
    expect(useToastStore.getState().toasts.some((toast) => toast.id === ids[0])).toBe(false);
  });

  it("dismiss closes a toast but keeps it for its exit animation", () => {
    const id = showToast({ title: "Copied" });
    dismissToast(id);
    expect(useToastStore.getState().toasts).toEqual([expect.objectContaining({ id, open: false })]);
  });
});
