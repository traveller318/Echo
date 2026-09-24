/**
 * SOURCE OF TRUTH KEYWORDS: useEchoMutation test, mutation error toast test, toastOnError test, command write test
 * WHAT:  Verifies useEchoMutation passes its input to the command, resolves the command's output, shows a failure
 *        as an AppError toast, and stays quiet when `toastOnError` is false.
 * WHY:   Every write in the UI goes through this hook; a failure that neither toasts nor reaches the caller would
 *        be silent data loss from the user's point of view.
 * WHERE: Runs in the `web` Vitest project.
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CommandResult } from "@/lib/command";
import { createEchoQueryClient } from "@/lib/query-client";
import { useToastStore } from "@/stores/toast-store";
import { useEchoMutation } from "./use-echo-mutation";

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={createEchoQueryClient()}>{children}</QueryClientProvider>;
}

const failing = () => Promise.resolve<CommandResult<null>>({ status: "error", error: { code: "Busy" } });

beforeEach(() => {
  useToastStore.setState({ toasts: [], nextId: 1 });
});

describe("useEchoMutation", () => {
  it("runs the command with the input and resolves its output", async () => {
    const command = vi.fn((input: number) => Promise.resolve<CommandResult<number>>({ status: "ok", data: input * 2 }));
    const { result } = renderHook(() => useEchoMutation(command), { wrapper });
    let output = 0;
    await act(async () => {
      output = await result.current.mutateAsync(21);
    });
    expect(command).toHaveBeenCalledWith(21);
    expect(output).toBe(42);
  });

  it("shows a failure as an AppError toast", async () => {
    const { result } = renderHook(() => useEchoMutation(failing), { wrapper });
    act(() => {
      result.current.mutate();
    });
    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });
    expect(useToastStore.getState().toasts).toEqual([
      expect.objectContaining({ title: "Already in progress", tone: "error" }),
    ]);
  });

  it("stays quiet when toastOnError is false", async () => {
    const { result } = renderHook(() => useEchoMutation(failing, { toastOnError: false }), { wrapper });
    act(() => {
      result.current.mutate();
    });
    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });
    expect(useToastStore.getState().toasts).toEqual([]);
  });
});
