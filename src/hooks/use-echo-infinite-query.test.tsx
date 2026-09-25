/**
 * SOURCE OF TRUTH KEYWORDS: useEchoInfiniteQuery test, cursor paging test, flattened items test, event refetch of pages
 * WHAT:  Verifies useEchoInfiniteQuery: the first page is read with no cursor, the next with the cursor Rust
 *        returned, rows are flattened in page order, the last page ends paging, and a declared Rust event re-reads
 *        every loaded page from the top.
 * WHY:   History pages through 100k+ takes with this hook and stays fresh only through events (02 §2.4).
 * WHERE: Runs in the `web` Vitest project with the invalidation bridge on an injected subscribe (no Tauri).
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type { Page, PageCursor } from "@/bindings";
import type { CommandResult } from "@/lib/command";
import type { EchoEventName } from "@/lib/echo-events";
import { createEchoQueryClient } from "@/lib/query-client";
import { startQueryInvalidation } from "@/lib/query-invalidation";
import { useEchoInfiniteQuery, type EchoInfiniteQuery } from "./use-echo-infinite-query";

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
  return { handlers, stop, wrapper };
}

/** Pages of two numbers over `rows`, keyed by the cursor of the last number shown. */
function pager(rows: () => readonly number[]) {
  return vi.fn((cursor: PageCursor | null): Promise<CommandResult<Page<number>>> => {
    const all = rows();
    const start = cursor === null ? 0 : all.indexOf(Number(cursor)) + 1;
    const items = all.slice(start, start + 2);
    const last = items.at(-1);
    const next_cursor = start + 2 < all.length && last !== undefined ? String(last) : null;
    return Promise.resolve({ status: "ok", data: { items, next_cursor } });
  });
}

describe("useEchoInfiniteQuery", () => {
  it("reads page after page with Rust's cursor and flattens the rows", async () => {
    const { wrapper, stop } = setup();
    const command = pager(() => [5, 4, 3, 2, 1]);
    const query: EchoInfiniteQuery<number> = { queryKey: ["numbers"], command };
    const { result } = renderHook(() => useEchoInfiniteQuery(query), { wrapper });
    await waitFor(() => {
      expect(result.current.items).toEqual([5, 4]);
    });
    expect(command).toHaveBeenLastCalledWith(null);
    expect(result.current.hasNextPage).toBe(true);

    await act(() => result.current.fetchNextPage());
    await waitFor(() => {
      expect(result.current.items).toEqual([5, 4, 3, 2]);
    });
    await act(() => result.current.fetchNextPage());
    await waitFor(() => {
      expect(result.current.items).toEqual([5, 4, 3, 2, 1]);
    });
    expect(command).toHaveBeenCalledWith("4");
    expect(command).toHaveBeenCalledWith("2");
    expect(result.current.hasNextPage).toBe(false);
    stop();
  });

  it("re-reads every loaded page from the top when a declared event arrives", async () => {
    const { wrapper, handlers, stop } = setup();
    let rows: readonly number[] = [4, 3, 2, 1];
    const command = pager(() => rows);
    const query: EchoInfiniteQuery<number> = { queryKey: ["live"], command, invalidatedBy: ["historyChanged"] };
    const { result } = renderHook(() => useEchoInfiniteQuery(query), { wrapper });
    await waitFor(() => {
      expect(result.current.items).toEqual([4, 3]);
    });
    await act(() => result.current.fetchNextPage());
    await waitFor(() => {
      expect(result.current.items).toEqual([4, 3, 2, 1]);
    });

    rows = [5, 4, 3, 2, 1];
    act(() => {
      handlers.get("historyChanged")?.();
    });
    await waitFor(() => {
      expect(result.current.items).toEqual([5, 4, 3, 2]);
    });
    stop();
  });
});
