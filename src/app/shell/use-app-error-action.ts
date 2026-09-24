/**
 * SOURCE OF TRUTH KEYWORDS: useAppErrorAction, AppErrorActionId handler, ACTION_TARGETS, open_logs, open_mic_privacy, open_settings, error action navigation
 * WHAT:  `useAppErrorAction()` returns the function that performs an AppError action id: the page actions
 *        (`open_history`, `open_models`, `open_settings`) navigate to that page's registry route, the system actions
 *        (`open_logs`, `open_mic_privacy`) run their command (`app_open_logs_dir`, `app_open_mic_privacy_settings`).
 * WHY:   lib/app-error.ts names actions by id so the copy table stays free of routing (its header says the shell
 *        decides); this is that decision, in one table keyed by the AppErrorActionId union, so a new action fails
 *        tsc until it does something. Pages are named by NavId and their route comes from the registry, so no path
 *        is written twice. A command that fails reports through the same toast surface.
 * WHERE: app/shell/Toaster.tsx (toast action buttons), app/shell/RouteError.tsx; needs the router and the
 *        registry context, so it is used only inside the routed shell.
 */
import { useCallback } from "react";
import { useNavigate } from "react-router";
import { commands, type NavId } from "@/bindings";
import { useRegistryView } from "@/hooks/use-registry";
import { toAppError, type AppErrorActionId } from "@/lib/app-error";
import { runCommand, type CommandResult } from "@/lib/command";
import { showAppErrorToast } from "@/stores/toast-store";

type ActionTarget =
  | { readonly kind: "page"; readonly page: NavId }
  | { readonly kind: "command"; readonly command: () => Promise<CommandResult<null>> };

const ACTION_TARGETS: Readonly<Record<AppErrorActionId, ActionTarget>> = {
  open_history: { kind: "page", page: "history" },
  open_models: { kind: "page", page: "models" },
  open_settings: { kind: "page", page: "settings" },
  open_logs: { kind: "command", command: commands.appOpenLogsDir },
  open_mic_privacy: { kind: "command", command: commands.appOpenMicPrivacySettings },
};

export function useAppErrorAction(): (id: AppErrorActionId) => void {
  const navigate = useNavigate();
  const { nav } = useRegistryView();
  return useCallback(
    (id: AppErrorActionId) => {
      const target = ACTION_TARGETS[id];
      if (target.kind === "command") {
        runCommand(target.command).catch((error: unknown) => {
          showAppErrorToast(toAppError(error));
        });
        return;
      }
      const item = nav.find((entry) => entry.id === target.page);
      if (item === undefined) {
        console.error("Echo has no page for this action", id);
        return;
      }
      void navigate(item.route);
    },
    [navigate, nav],
  );
}
