/**
 * SOURCE OF TRUTH KEYWORDS: HistoryPage, history route, history page, search takes, take list, retry take, delete take, clear history
 * WHAT:  The History screen (04 §5): a full-text search field over a virtualized DataList of every take (time,
 *        two-line preview, duration, status badge), row actions Copy / Retry / Delete on hover or focus, a
 *        confirmation before a delete, and "Clear history" in the page header (confirmed first).
 * WHY:   All data comes from Rust through useHistoryList and stays fresh from HistoryChanged and TranscriptSaved; the
 *        page keeps only UI state (the search text, whether the clear confirmation is open, and through
 *        useTakeInspector which take is being deleted), never a copy of the takes (root CLAUDE.md §7). A row click
 *        opens nothing: the row and its actions are the whole take UI (decision 2026-09-26). Clearing erases every
 *        take's text and audio but Rust keeps their measurements, so the dashboard and streak survive; the button is
 *        off while there is visibly nothing to clear. The row actions and confirmation are the shared
 *        components/global/take-actions, also used by the Dashboard. Pages load 100 rows at a time as the list nears
 *        its end. The row clock is the list's last refresh (`dataUpdatedAt`), so "today" is judged once per
 *        refresh, not per row. The search limit is the one Rust enforces (HISTORY_SEARCH_MAX_CHARS, generated).
 *        Loading shows an indeterminate bar only after --delay-loading (ProgressBar), so a fast read never flashes.
 * WHERE: Lazy-loaded by app/routes.tsx for the `history` nav entry (app/nav-page.ts).
 */
import { SearchXIcon, Trash2Icon } from "lucide-react";
import { useCallback, useState } from "react";
import { HISTORY_SEARCH_MAX_CHARS, type TranscriptSummary } from "@/bindings";
import {
  ConfirmDialog,
  DataList,
  EmptyState,
  NavIcon,
  Page,
  ProgressBar,
  TakeOverlays,
  TakeRowActions,
  TranscriptRow,
  useTakeInspector,
} from "@/components/global";
import { Button } from "@/components/ui";
import type { NavPageProps } from "@/app/nav-page";
import { useClearHistory, useHistoryList } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";

export default function HistoryPage({ nav }: NavPageProps) {
  const [search, setSearch] = useState("");
  const list = useHistoryList(search);
  const takes = useTakeInspector();
  const clear = useClearHistory();
  const [confirmingClear, setConfirmingClear] = useState(false);
  const { fetchNextPage, isFetchingNextPage } = list;
  const nothingToClear = search.trim() === "" && list.items.length === 0;

  const loadMore = useCallback(() => {
    if (!isFetchingNextPage) {
      void fetchNextPage();
    }
  }, [fetchNextPage, isFetchingNextPage]);

  const empty = list.isPending ? (
    <ProgressBar value={null} aria-label="Loading your takes" />
  ) : list.isError ? (
    <EmptyState
      icon={<NavIcon icon={nav.icon} />}
      title={describeAppError(toAppError(list.error)).title}
      body={describeAppError(toAppError(list.error)).body}
      action={
        <Button
          onClick={() => {
            void list.refetch();
          }}
        >
          Try again
        </Button>
      }
    />
  ) : search.trim() === "" ? (
    <EmptyState
      icon={<NavIcon icon={nav.icon} />}
      title="No takes yet"
      body="Press your dictation hotkey and speak. Every take is saved here, with its audio."
    />
  ) : (
    <EmptyState icon={<SearchXIcon />} title="No takes match" body="Try fewer or different words." />
  );

  return (
    <Page
      title={nav.label}
      className="h-full"
      actions={
        <Button
          variant="ghost"
          size="sm"
          disabled={nothingToClear || clear.isPending}
          aria-busy={clear.isPending}
          onClick={() => {
            setConfirmingClear(true);
          }}
        >
          <Trash2Icon aria-hidden="true" />
          Clear history
        </Button>
      }
    >
      <DataList<TranscriptSummary>
        className="flex-1"
        label="Takes"
        items={list.items}
        getKey={(take) => take.id}
        row={(take) => <TranscriptRow take={take} now={list.dataUpdatedAt} />}
        actions={(take) => <TakeRowActions take={take} actions={takes.actions} onDelete={takes.askDelete} />}
        empty={empty}
        search={{
          value: search,
          onChange: setSearch,
          label: "Search takes",
          placeholder: "Search your takes",
          maxLength: HISTORY_SEARCH_MAX_CHARS,
        }}
        hasMore={list.hasNextPage}
        loadingMore={isFetchingNextPage}
        onEndReached={loadMore}
        footer={isFetchingNextPage ? <ProgressBar value={null} aria-label="Loading more takes" /> : null}
      />
      <TakeOverlays inspector={takes} />
      <ConfirmDialog
        open={confirmingClear}
        title="Clear all history?"
        description="Every take's text and saved audio are removed. Your dashboard numbers and streak stay. This can't be undone."
        confirmLabel="Clear history"
        onCancel={() => {
          setConfirmingClear(false);
        }}
        onConfirm={() => {
          setConfirmingClear(false);
          clear.run();
        }}
      />
    </Page>
  );
}
