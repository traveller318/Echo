/**
 * SOURCE OF TRUTH KEYWORDS: HistoryPage, history route, history page, search takes, take list, detail sheet, retry take, delete take
 * WHAT:  The History screen (04 §5): a full-text search field over a virtualized DataList of every take (time,
 *        two-line preview, duration, status badge), row actions Copy / Retry / Delete on hover or focus, a detail
 *        Sheet with the full text, and a confirmation before a delete.
 * WHY:   All data comes from Rust through useHistoryList / useTranscript and stays fresh from HistoryChanged and
 *        TranscriptSaved; the page keeps only UI state (the search text, which take is open, which is being
 *        deleted), never a copy of the takes (root CLAUDE.md §7). Pages load 100 rows at a time as the list nears
 *        its end. The row clock is the list's last refresh (`dataUpdatedAt`), so "today" is judged once per
 *        refresh, not per row. The search limit is the one Rust enforces (HISTORY_SEARCH_MAX_CHARS, generated).
 *        Loading shows an indeterminate bar only after --delay-loading (ProgressBar), so a fast read never flashes.
 * WHERE: Lazy-loaded by app/routes.tsx for the `history` nav entry (app/nav-page.ts).
 */
import { SearchXIcon } from "lucide-react";
import { useCallback, useState } from "react";
import { HISTORY_SEARCH_MAX_CHARS, type TranscriptId, type TranscriptSummary } from "@/bindings";
import { DataList, EmptyState, NavIcon, Page, ProgressBar, TranscriptRow } from "@/components/global";
import { Button } from "@/components/ui";
import type { NavPageProps } from "@/app/nav-page";
import { useHistoryList, useTranscriptActions } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";
import { DeleteTakeDialog } from "./_components/DeleteTakeDialog";
import { HistoryRowActions } from "./_components/HistoryRowActions";
import { TranscriptSheet } from "./_components/TranscriptSheet";

export default function HistoryPage({ nav }: NavPageProps) {
  const [search, setSearch] = useState("");
  const [openId, setOpenId] = useState<TranscriptId | null>(null);
  const [deleting, setDeleting] = useState<TranscriptId | null>(null);
  const list = useHistoryList(search);
  const actions = useTranscriptActions();
  const { fetchNextPage, isFetchingNextPage } = list;

  const loadMore = useCallback(() => {
    if (!isFetchingNextPage) {
      void fetchNextPage();
    }
  }, [fetchNextPage, isFetchingNextPage]);
  const closeSheet = useCallback(() => {
    setOpenId(null);
  }, []);
  const askDelete = useCallback((take: { readonly id: TranscriptId }) => {
    setDeleting(take.id);
  }, []);

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
    <Page title={nav.label} className="h-full">
      <DataList<TranscriptSummary>
        className="flex-1"
        label="Takes"
        items={list.items}
        getKey={(take) => take.id}
        row={(take) => <TranscriptRow take={take} now={list.dataUpdatedAt} />}
        actions={(take) => <HistoryRowActions take={take} actions={actions} onDelete={askDelete} />}
        empty={empty}
        onActivate={(take) => {
          setOpenId(take.id);
        }}
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
      <TranscriptSheet id={openId} onClose={closeSheet} actions={actions} onDelete={askDelete} />
      <DeleteTakeDialog
        take={deleting}
        onCancel={() => {
          setDeleting(null);
        }}
        onConfirm={(id) => {
          setDeleting(null);
          actions.remove.run(id, () => {
            if (openId === id) {
              setOpenId(null);
            }
          });
        }}
      />
    </Page>
  );
}
