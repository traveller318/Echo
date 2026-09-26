/**
 * SOURCE OF TRUTH KEYWORDS: useHistoryList, useTranscript, useTranscriptActions, useClearHistory, useRecentTakes, historyListQuery, recentTakesQuery, transcriptQuery, HISTORY_EVENTS, history_list, session_retry, history_copy, history_delete, history_clear
 * WHAT:  The History data layer: `historyListQuery(search)` / `useHistoryList(search)` (paged takes, newest
 *        first, optionally searched), `recentTakesQuery(limit)` / `useRecentTakes(limit)` (the newest few takes),
 *        `transcriptQuery(id)` / `useTranscript(id)` (one take in full), `useTranscriptActions()` (copy, retry and
 *        delete as mutations with their success toasts) and `useClearHistory()` (clear every take from History).
 * WHY:   Every surface that shows takes (History, the Dashboard's recent takes) reads and acts through these, so
 *        keys, events and copy cannot drift. Reads are invalidated by HistoryChanged and TranscriptSaved (02 §4.4)
 *        and never polled; a write changes nothing in the cache by hand, its Rust event refetches (root CLAUDE.md
 *        §7). The recent takes also refresh on MetricsChanged, which Rust sends at local midnight, so "today" in
 *        their times moves on with the day. Failures toast through the AppError copy table (useEchoMutation);
 *        successes toast here in calm copy (04 §1). The page size is well under Rust's HistoryListInput limit.
 * WHERE: routes/history (list, clear), routes/dashboard (recent takes), components/global/take-actions (row
 *        actions), routes/onboarding PracticePad (one take in full).
 */
import {
  commands,
  type Page,
  type PageCursor,
  type Transcript,
  type TranscriptId,
  type TranscriptSummary,
} from "@/bindings";
import type { EchoEventName } from "@/lib/echo-events";
import { showToast } from "@/stores/toast-store";
import { useEchoInfiniteQuery, type EchoInfiniteQuery } from "./use-echo-infinite-query";
import { useEchoMutation } from "./use-echo-mutation";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";

/** Rust events after which any History read is stale. */
export const HISTORY_EVENTS: readonly EchoEventName[] = ["historyChanged", "transcriptSaved"];

/** Rows per History page. */
export const HISTORY_PAGE_SIZE = 100;

/** The History list for `search` (blank = every take). */
export function historyListQuery(search: string): EchoInfiniteQuery<TranscriptSummary> {
  const text = search.trim();
  return {
    queryKey: ["history", "list", text],
    command: (cursor: PageCursor | null) =>
      commands.historyList({ search: text === "" ? null : text, cursor, limit: HISTORY_PAGE_SIZE }),
    invalidatedBy: HISTORY_EVENTS,
  };
}

/** The newest `limit` takes of any status, newest first (one page, no search). */
export function recentTakesQuery(limit: number): EchoQuery<Page<TranscriptSummary>> {
  return {
    queryKey: ["history", "recent", limit],
    command: () => commands.historyList({ search: null, cursor: null, limit }),
    invalidatedBy: [...HISTORY_EVENTS, "metricsChanged"],
  };
}

/** The newest `limit` takes. */
export function useRecentTakes(limit: number) {
  return useEchoQuery(recentTakesQuery(limit), { select: selectItems });
}

function selectItems(page: Page<TranscriptSummary>): TranscriptSummary[] {
  return page.items;
}

/** One take in full. */
export function transcriptQuery(id: TranscriptId): EchoQuery<Transcript> {
  return {
    queryKey: ["history", "take", id],
    command: () => commands.historyGet({ id }),
    invalidatedBy: HISTORY_EVENTS,
  };
}

export function useHistoryList(search: string) {
  return useEchoInfiniteQuery(historyListQuery(search));
}

/** The take `id` in full; reads nothing while `id` is null. */
export function useTranscript(id: TranscriptId | null) {
  // A placeholder key keeps the hook order stable while nothing is selected; it never runs.
  return useEchoQuery(transcriptQuery(id ?? ""), { enabled: id !== null });
}

export interface TranscriptActions {
  readonly copy: ReturnType<typeof useCopyTake>;
  readonly retry: ReturnType<typeof useRetryTake>;
  readonly remove: ReturnType<typeof useDeleteTake>;
}

function useCopyTake() {
  const mutation = useEchoMutation(commands.historyCopy);
  return {
    ...mutation,
    run: (id: TranscriptId) => {
      mutation.mutate({ id }, { onSuccess: () => showToast({ title: "Copied to the clipboard" }) });
    },
  };
}

function useRetryTake() {
  const mutation = useEchoMutation(commands.sessionRetry);
  return {
    ...mutation,
    run: (id: TranscriptId) => {
      mutation.mutate(
        { id },
        {
          onSuccess: (row) => {
            showToast(
              row.status === "done"
                ? { title: "Transcribed again", body: "The new text is in History." }
                : { title: "No speech found", body: "The saved audio holds no words to transcribe." },
            );
          },
        },
      );
    },
  };
}

function useDeleteTake() {
  const mutation = useEchoMutation(commands.historyDelete);
  return {
    ...mutation,
    run: (id: TranscriptId) => {
      mutation.mutate({ id }, { onSuccess: () => showToast({ title: "Take deleted" }) });
    },
  };
}

/**
 * SOURCE OF TRUTH KEYWORDS: useClearHistory, clear history mutation, history_clear, History cleared toast
 * WHAT:  Clears every take from History (`history_clear`) and toasts once it is done.
 * WHY:   Rust erases the text and audio but keeps each take's measurements, so the toast can promise the dashboard
 *        and streak are untouched. The list refreshes from HistoryChanged { cleared }, never from this mutation.
 * WHERE: routes/history (the page header's Clear history).
 */
export function useClearHistory() {
  const mutation = useEchoMutation(commands.historyClear);
  return {
    ...mutation,
    run: () => {
      mutation.mutate(undefined, {
        onSuccess: () => showToast({ title: "History cleared", body: "Your dashboard and streak are unchanged." }),
      });
    },
  };
}

/** Copy, retry and delete for any take; `pending` state is per mutation (one of each runs at a time). */
export function useTranscriptActions(): TranscriptActions {
  return { copy: useCopyTake(), retry: useRetryTake(), remove: useDeleteTake() };
}
