/**
 * SOURCE OF TRUTH KEYWORDS: createCommandQueue, CommandQueue, ordered command calls, fire and forget command, sequential invoke
 * WHAT:  `createCommandQueue(purpose)` returns a `send(call)` that runs generated command calls one after another,
 *        in the order they were sent, and logs a failure (with `purpose`) instead of throwing.
 * WHY:   Tauri may run two quick invokes concurrently, so a "switch on" and the "switch off" right after it could land
 *        in the wrong order; state that a component switches on mount and off on unmount (a StrictMode remount sends
 *        on, off, on) must end on the last call sent. The calls it carries are switches Rust also guards (a rehearsal
 *        only applies while Echo has focus, a capture lease runs out), so a lost call is logged, never thrown into a
 *        React effect.
 * WHERE: hooks/use-session-rehearsal.ts (the onboarding rehearsal), hooks/use-hotkey-status.ts (the capture lease).
 */
import { runCommand, type CommandResult } from "./command";

/** Sends one command call after every call sent before it has settled. */
export type CommandQueue = (call: () => Promise<CommandResult<unknown>>) => void;

export function createCommandQueue(purpose: string): CommandQueue {
  let pending: Promise<void> = Promise.resolve();
  return (call) => {
    pending = pending
      .then(() => runCommand(call))
      .then(
        () => undefined,
        (error: unknown) => {
          console.error(`Echo could not ${purpose}`, error);
        },
      );
  };
}
