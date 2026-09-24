/**
 * SOURCE OF TRUTH KEYWORDS: useEchoMutation, command write, TanStack useMutation, error toast, CommandError, settings_set
 * WHAT:  `useEchoMutation(command, { toastOnError })` wraps a generated command that changes something (e.g.
 *        `commands.settingsSet`) in a TanStack mutation: `mutate(input)` runs it, data is the command output,
 *        and a failure is a CommandError that is also shown as an AppError toast unless `toastOnError` is false.
 * WHY:   Writes never update cached data by hand: the command emits its Rust event (SettingsChanged,
 *        HistoryChanged…) and the queries that named that event refetch, so the UI keeps no second copy (root
 *        CLAUDE.md §7). Failures surface through the one error copy table by default, so no caller forgets them;
 *        a form that shows the error inline (a Validation on a field) turns the toast off.
 * WHERE: Settings controls (step 18), History actions (step 16), Models actions (step 21).
 */
import { useMutation, type UseMutationResult } from "@tanstack/react-query";
import { toAppError } from "@/lib/app-error";
import { runCommand, type CommandResult } from "@/lib/command";
import { showAppErrorToast } from "@/stores/toast-store";

export interface UseEchoMutationOptions {
  /** Show a failure as an AppError toast (default true). */
  readonly toastOnError?: boolean;
}

export function useEchoMutation<I, T>(
  command: (input: I) => Promise<CommandResult<T>>,
  { toastOnError = true }: UseEchoMutationOptions = {},
): UseMutationResult<T, Error, I> {
  return useMutation({
    mutationFn: (input: I) => runCommand(() => command(input)),
    onError: (error) => {
      if (toastOnError) {
        showAppErrorToast(toAppError(error));
      }
    },
  });
}
