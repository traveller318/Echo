/**
 * SOURCE OF TRUTH KEYWORDS: useTakeInspector, TakeInspector, open take, detail sheet state, delete confirmation state, take actions wiring
 * WHAT:  The UI state every list of takes needs: which take's detail sheet is open, which take is waiting for its
 *        delete confirmation, and the shared copy / retry / delete mutations; plus the moves between those states
 *        (open, close, ask to delete, cancel, confirm).
 * WHY:   History and the Dashboard's recent takes offer the same actions, sheet and confirmation (04 §5), so the
 *        wiring lives once instead of in each page (root CLAUDE.md §7). It holds UI state only: the takes
 *        themselves stay in the query cache, fed by Rust events. A confirmed delete closes the sheet only when it
 *        still shows the deleted take (a functional update, so a sheet opened meanwhile stays open).
 * WHERE: routes/history, routes/dashboard; read by TakeOverlays and the TakeRowActions slot (this folder).
 */
import { useCallback, useState } from "react";
import type { TranscriptId } from "@/bindings";
import { useTranscriptActions, type TranscriptActions } from "@/hooks";

export interface TakeInspector {
  readonly actions: TranscriptActions;
  /** The take whose detail sheet is open. */
  readonly openId: TranscriptId | null;
  /** The take waiting for its delete confirmation. */
  readonly deletingId: TranscriptId | null;
  readonly open: (id: TranscriptId) => void;
  readonly close: () => void;
  readonly askDelete: (take: { readonly id: TranscriptId }) => void;
  readonly cancelDelete: () => void;
  readonly confirmDelete: (id: TranscriptId) => void;
}

export function useTakeInspector(): TakeInspector {
  const [openId, setOpenId] = useState<TranscriptId | null>(null);
  const [deletingId, setDeletingId] = useState<TranscriptId | null>(null);
  const actions = useTranscriptActions();
  const { run: remove } = actions.remove;

  const open = useCallback((id: TranscriptId) => {
    setOpenId(id);
  }, []);
  const close = useCallback(() => {
    setOpenId(null);
  }, []);
  const askDelete = useCallback((take: { readonly id: TranscriptId }) => {
    setDeletingId(take.id);
  }, []);
  const cancelDelete = useCallback(() => {
    setDeletingId(null);
  }, []);
  const confirmDelete = useCallback(
    (id: TranscriptId) => {
      setDeletingId(null);
      remove(id, () => {
        setOpenId((current) => (current === id ? null : current));
      });
    },
    [remove],
  );

  return { actions, openId, deletingId, open, close, askDelete, cancelDelete, confirmDelete };
}
