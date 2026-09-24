/**
 * SOURCE OF TRUTH KEYWORDS: useAppErrorAction, AppErrorActionId handler, error action navigation, error action in main window
 * WHAT:  `useAppErrorAction()` returns the function that performs an AppError action id in the main window: page
 *        actions navigate the router to that page, command actions run their command (`app_open_logs_dir`,
 *        `app_open_mic_privacy_settings`).
 * WHY:   What each id means is decided once in lib/app-error-actions.ts (shared with the pill); the main window only
 *        supplies how a page opens here (its router, through useOpenPage). A command that fails reports through the
 *        same toast surface.
 * WHERE: app/shell/Toaster.tsx (toast action buttons), app/shell/RouteError.tsx; needs the router and the
 *        registry context, so it is used only inside the routed shell.
 */
import { useCallback } from "react";
import type { AppErrorActionId } from "@/lib/app-error";
import { performAppErrorAction } from "@/lib/app-error-actions";
import { showAppErrorToast } from "@/stores/toast-store";
import { useOpenPage } from "./use-open-page";

export function useAppErrorAction(): (id: AppErrorActionId) => void {
  const openPage = useOpenPage();
  return useCallback(
    (id: AppErrorActionId) => {
      performAppErrorAction(id, { openPage, onError: showAppErrorToast });
    },
    [openPage],
  );
}
