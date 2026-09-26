/**
 * SOURCE OF TRUTH KEYWORDS: createCommandQueue test, ordered command calls test, command failure logged test
 * WHAT:  Verifies that a command queue starts each call only after the previous one settled, in send order, and that
 *        a failed call is logged and does not stop the calls after it.
 * WHY:   The rehearsal and the hotkey capture lease depend on "last sent wins"; a reordered on/off would leave
 *        Echo's hotkeys off or a rehearsal on.
 * WHERE: Runs in the `web` Vitest project.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CommandResult } from "./command";
import { createCommandQueue } from "./command-queue";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("createCommandQueue", () => {
  it("runs calls one after another in send order and keeps going after a failure", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const send = createCommandQueue("test the queue");
    const started: string[] = [];
    let releaseFirst: (result: CommandResult<null>) => void = () => undefined;
    const first = new Promise<CommandResult<null>>((resolve) => {
      releaseFirst = resolve;
    });
    send(() => {
      started.push("first");
      return first;
    });
    send(() => {
      started.push("second");
      return Promise.resolve({ status: "error", error: { code: "Internal" } });
    });
    const third = new Promise<void>((resolve) => {
      send(() => {
        started.push("third");
        resolve();
        return Promise.resolve({ status: "ok", data: null });
      });
    });
    await Promise.resolve();
    expect(started).toEqual(["first"]);
    releaseFirst({ status: "ok", data: null });
    await third;
    expect(started).toEqual(["first", "second", "third"]);
    expect(error).toHaveBeenCalledWith("Echo could not test the queue", expect.anything());
  });
});
