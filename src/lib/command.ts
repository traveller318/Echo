/**
 * SOURCE OF TRUTH KEYWORDS: runCommand, CommandResult, unwrap command result, command call, typedError result, CommandError
 * WHAT:  `runCommand(call)` invokes one generated command and resolves to its data, or rejects with a CommandError
 *        that carries the AppError (the command's own, or `Internal` when the call itself failed).
 * WHY:   The generated bindings resolve to `{ status, data | error }` and can still throw when the webview bridge
 *        fails (05 W27); callers that need a plain promise (TanStack Query, window actions) get one shape here
 *        instead of each re-implementing the unwrap. `CommandResult` is the readonly shape every generated command
 *        satisfies, so any command can be passed without a cast.
 * WHERE: hooks/use-echo-query.ts, hooks/use-echo-mutation.ts, the app shell's error actions.
 */
import type { AppError } from "@/bindings";
import { CommandError, toAppError } from "./app-error";

/** What every generated command resolves to. */
export type CommandResult<T> =
  | { readonly status: "ok"; readonly data: T }
  | { readonly status: "error"; readonly error: AppError };

export async function runCommand<T>(call: () => Promise<CommandResult<T>>): Promise<T> {
  let result: CommandResult<T>;
  try {
    result = await call();
  } catch (error: unknown) {
    throw new CommandError(toAppError(error));
  }
  if (result.status === "error") {
    throw new CommandError(result.error);
  }
  return result.data;
}
