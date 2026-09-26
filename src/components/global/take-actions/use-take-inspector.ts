/**
 * SOURCE OF TRUTH KEYWORDS: useTakeInspector, TakeInspector, delete confirmation state, take actions wiring
 * WHAT:  The UI state every list of takes needs: which take is waiting for its delete confirmation, and the shared
 *        copy / retry / delete mutations; plus the moves between those states (ask to delete, cancel, confirm).
 * WHY:   History and the Dashboard's recent takes offer the same actions and confirmation (04 §5), so the wiring
 *        lives once instead of in each page (root CLAUDE.md §7). It holds UI state only: the takes themselves stay
 *        in the query cache, fed by Rust events.
 * WHERE: routes/history, routes/dashboard; read by TakeOverlays and the TakeRowActions slot (this folder).
 */
import { useCallback, useState } from "react";
import type { TranscriptId } from "@/bindings";
import { useTranscriptActions, type TranscriptActions } from "@/hooks";

export interface TakeInspector {
  readonly actions: TranscriptActions;
  /** The take waiting for its delete confirmation. */
  readonly deletingId: TranscriptId | null;
  readonly askDelete: (take: { readonly id: TranscriptId }) => void;
  readonly cancelDelete: () => void;
  readonly confirmDelete: (id: TranscriptId) => void;
}

export function useTakeInspector(): TakeInspector {
  const [deletingId, setDeletingId] = useState<TranscriptId | null>(null);
  const actions = useTranscriptActions();
  const { run: remove } = actions.remove;

  const askDelete = useCallback((take: { readonly id: TranscriptId }) => {
    setDeletingId(take.id);
  }, []);
  const cancelDelete = useCallback(() => {
    setDeletingId(null);
  }, []);
  const confirmDelete = useCallback(
    (id: TranscriptId) => {
      setDeletingId(null);
      remove(id);
    },
    [remove],
  );

  return { actions, deletingId, askDelete, cancelDelete, confirmDelete };
}
