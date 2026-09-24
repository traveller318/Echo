/**
 * SOURCE OF TRUTH KEYWORDS: APP_ERROR_ACTION_TARGETS, performAppErrorAction, AppErrorActionTarget, error action page, error action command, open_history, open_models
 * WHAT:  What each AppError action id does (`APP_ERROR_ACTION_TARGETS`: open a main-window page, or run a command)
 *        and `performAppErrorAction(id, { openPage, onError })`, which does it.
 * WHY:   lib/app-error.ts names actions by id so its copy table stays free of routing; the windows decide how to
 *        open a page (the main window navigates its router, the pill asks Rust to bring the main window forward
 *        with `app_open_page`), but what each id means is decided once, here, keyed by the AppErrorActionId union so
 *        a new action fails tsc until it does something. Pages are named by NavId; their routes come from the
 *        registry where the router lives. A command that fails is reported through `onError`, never thrown.
 * WHERE: app/shell/use-app-error-action.ts (toasts, page errors in the main window) and the pill (src/pill: the
 *        error state's "Open").
 */
import { commands, type AppError, type NavId } from "@/bindings";
import type { AppErrorActionId } from "./app-error";
import { toAppError } from "./app-error";
import { runCommand, type CommandResult } from "./command";

export type AppErrorActionTarget =
  | { readonly kind: "page"; readonly page: NavId }
  | { readonly kind: "command"; readonly command: () => Promise<CommandResult<null>> };

export const APP_ERROR_ACTION_TARGETS: Readonly<Record<AppErrorActionId, AppErrorActionTarget>> = {
  open_history: { kind: "page", page: "history" },
  open_models: { kind: "page", page: "models" },
  open_settings: { kind: "page", page: "settings" },
  open_logs: { kind: "command", command: commands.appOpenLogsDir },
  open_mic_privacy: { kind: "command", command: commands.appOpenMicPrivacySettings },
};

export interface PerformAppErrorActionOptions {
  /** Shows a main-window page (navigation in the main window, `app_open_page` elsewhere). */
  readonly openPage: (page: NavId) => void;
  /** Reports a command that failed. */
  readonly onError: (error: AppError) => void;
}

export function performAppErrorAction(id: AppErrorActionId, { openPage, onError }: PerformAppErrorActionOptions): void {
  const target = APP_ERROR_ACTION_TARGETS[id];
  if (target.kind === "page") {
    openPage(target.page);
    return;
  }
  runCommand(target.command).catch((error: unknown) => {
    onError(toAppError(error));
  });
}
