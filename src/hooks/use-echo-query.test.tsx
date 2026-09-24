/**
 * SOURCE OF TRUTH KEYWORDS: useEchoQuery test, event invalidation refetch test, CommandError query test, select test
 * WHAT:  Verifies useEchoQuery resolves a command's data (through `select`), surfaces a failure as a CommandError
 *        with the command's AppError, refetches when a declared Rust event arrives, and never reads while disabled.
 * WHY:   Every screen reads Rust through this hook; the event-driven refetch is the only thing keeping it fresh.
 * WHERE: Runs in the `web` Vitest project with the invalidation bridge on an injected subscribe (no Tauri).
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { toAppError } from "@/lib/app-error";
import type { CommandResult } from "@/lib/command";
import type { EchoEventName } from "@/lib/echo-events";
import { createEchoQueryClient } from "@/lib/query-client";
import { startQueryInvalidation } from "@/lib/query-invalidation";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";

function setup() {
  const client = createEchoQueryClient();
  const handlers = new Map<EchoEventName, () => void>();
  const stop = startQueryInvalidation(client, (name, handler) => {
    handlers.set(name, handler);
    return () => handlers.delete(name);
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, handlers, stop, wrapper };
}

describe("useEchoQuery", () => {
  it("resolves the command's data through select", async () => {
    const { wrapper, stop } = setup();
    const query: EchoQuery<number[]> = {
      queryKey: ["numbers"],
      command: () => Promise.resolve({ status: "ok", data: [1, 2, 3] }),
    };
    const { result } = renderHook(() => useEchoQuery(query, { select: (data) => data.length }), { wrapper });
    await waitFor(() => {
      expect(result.current.data).toBe(3);
    });
    stop();
  });

  it("surfaces a failed command as its AppError", async () => {
    const { wrapper, stop } = setup();
    const query: EchoQuery<number> = {
      queryKey: ["failing"],
      command: () => Promise.resolve({ status: "error", error: { code: "Storage" } }),
    };
    const { result } = renderHook(() => useEchoQuery(query), { wrapper });
    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });
    expect(toAppError(result.current.error)).toEqual({ code: "Storage" });
    stop();
  });

  it("reads again when a declared event arrives, and only then", async () => {
    const { wrapper, handlers, stop } = setup();
    let version = 0;
    const command = vi.fn(() => {
      version += 1;
      return Promise.resolve<CommandResult<number>>({ status: "ok", data: version });
    });
    const query: EchoQuery<number> = { queryKey: ["versioned"], command, invalidatedBy: ["settingsChanged"] };
    const { result } = renderHook(() => useEchoQuery(query), { wrapper });
    await waitFor(() => {
      expect(result.current.data).toBe(1);
    });
    expect([...handlers.keys()]).toEqual(["settingsChanged"]);

    handlers.get("settingsChanged")?.();
    await waitFor(() => {
      expect(result.current.data).toBe(2);
    });
    expect(command).toHaveBeenCalledTimes(2);
    stop();
  });

  it("does not read while disabled", () => {
    const { wrapper, stop } = setup();
    const command = vi.fn(() => Promise.resolve<CommandResult<number>>({ status: "ok", data: 1 }));
    const { result } = renderHook(() => useEchoQuery({ queryKey: ["off"], command }, { enabled: false }), {
      wrapper,
    });
    expect(result.current.fetchStatus).toBe("idle");
    expect(command).not.toHaveBeenCalled();
    stop();
  });
});
