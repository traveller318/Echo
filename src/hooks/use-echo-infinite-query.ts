/**
 * SOURCE OF TRUTH KEYWORDS: useEchoInfiniteQuery, echoInfiniteQueryOptions, EchoInfiniteQuery, cursor paging, Page next_cursor, infinite list, invalidatedBy
 * WHAT:  `echoInfiniteQueryOptions({ queryKey, command, invalidatedBy })` turns a generated command that returns a
 *        cursor-paginated `Page<T>` into TanStack infinite query options; `useEchoInfiniteQuery(query)` runs it and
 *        also hands back the loaded pages flattened into one `items` array.
 * WHY:   Long lists (History at 100k+ rows) are read a page at a time with the opaque cursor Rust returns
 *        (`next_cursor`, None on the last page), so the list never asks for an empty page. Like useEchoQuery, the
 *        query names the Rust events that make it stale in `meta.invalidatedBy` and is refetched only by them, never
 *        polled (02 §2.4); a refetch re-reads each loaded page from the first cursor on, so rows inserted at the top
 *        never cause gaps or duplicates. Previous data stays on screen while a new key (a new search) loads, so the
 *        list does not flash empty on every keystroke.
 * WHERE: hooks/use-history.ts (history_list); later any paged command.
 */
import {
  infiniteQueryOptions,
  keepPreviousData,
  useInfiniteQuery,
  type InfiniteData,
  type QueryKey,
  type UseInfiniteQueryResult,
} from "@tanstack/react-query";
import { useMemo } from "react";
import type { Page, PageCursor } from "@/bindings";
import { runCommand, type CommandResult } from "@/lib/command";
import type { EchoEventName } from "@/lib/echo-events";

export interface EchoInfiniteQuery<T> {
  readonly queryKey: QueryKey;
  /** Reads the page after `cursor` (null = the first page), e.g. `(cursor) => commands.historyList({ …, cursor })`. */
  readonly command: (cursor: PageCursor | null) => Promise<CommandResult<Page<T>>>;
  /** Rust events after which every loaded page is stale. */
  readonly invalidatedBy?: readonly EchoEventName[];
}

export function echoInfiniteQueryOptions<T>({ queryKey, command, invalidatedBy = [] }: EchoInfiniteQuery<T>) {
  return infiniteQueryOptions<Page<T>, Error, InfiniteData<Page<T>, PageCursor | null>, QueryKey, PageCursor | null>({
    queryKey,
    queryFn: ({ pageParam }) => runCommand(() => command(pageParam)),
    initialPageParam: null,
    getNextPageParam: (lastPage) => lastPage.next_cursor,
    meta: { invalidatedBy },
  });
}

export type UseEchoInfiniteQueryResult<T> = UseInfiniteQueryResult<InfiniteData<Page<T>, PageCursor | null>> & {
  /** Every loaded row, in page order. */
  readonly items: readonly T[];
};

export function useEchoInfiniteQuery<T>(query: EchoInfiniteQuery<T>): UseEchoInfiniteQueryResult<T> {
  const result = useInfiniteQuery({ ...echoInfiniteQueryOptions(query), placeholderData: keepPreviousData });
  const pages = result.data?.pages;
  const items = useMemo(() => pages?.flatMap((page) => page.items) ?? [], [pages]);
  return { ...result, items };
}
