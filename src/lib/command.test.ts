/**
 * SOURCE OF TRUTH KEYWORDS: runCommand test, CommandError test, toAppError CommandError test, command unwrap
 * WHAT:  Verifies runCommand resolves data, rejects with a CommandError carrying the command's AppError, and turns
 *        a failed call (the bridge threw) into `Internal`; toAppError unwraps a CommandError.
 * WHY:   Every query and mutation depends on this unwrap; a CommandError that lost its AppError would show the
 *        wrong copy for every failure.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import { CommandError, toAppError } from "./app-error";
import { runCommand } from "./command";

describe("runCommand", () => {
  it("resolves to the data of a successful command", async () => {
    await expect(runCommand(() => Promise.resolve({ status: "ok", data: 42 }))).resolves.toBe(42);
  });

  it("rejects with a CommandError that carries the command's AppError", async () => {
    const failure = runCommand(() =>
      Promise.resolve({ status: "error", error: { code: "NotFound", resource: "transcript" } }),
    );
    await expect(failure).rejects.toBeInstanceOf(CommandError);
    await failure.catch((error: unknown) => {
      expect(toAppError(error)).toEqual({ code: "NotFound", resource: "transcript" });
    });
  });

  it("turns a call that threw into Internal", async () => {
    const failure = runCommand(() => Promise.reject(new TypeError("no Tauri here")));
    await failure.catch((error: unknown) => {
      expect(error).toBeInstanceOf(CommandError);
      expect(toAppError(error)).toEqual({ code: "Internal" });
    });
    await expect(failure).rejects.toThrow("Echo command failed with Internal");
  });
});
