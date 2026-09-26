/**
 * SOURCE OF TRUTH KEYWORDS: DeleteTakeDialog, confirm delete take, destructive confirmation, delete audio and text
 * WHAT:  Asks before a take is deleted: its text and saved audio go for good. Delete runs the mutation and closes;
 *        Cancel (or Esc) keeps the take.
 * WHY:   00 constraint 5 "never lose a take": deleting is the one per-take action that cannot be undone, so it is
 *        confirmed through the shared ConfirmDialog (destructive button, Cancel focused first).
 * WHERE: TakeOverlays (this folder), from a row's Delete.
 */
import type { TranscriptId } from "@/bindings";
import { ConfirmDialog } from "../confirm-dialog";

export interface DeleteTakeDialogProps {
  /** The take to delete; null keeps the dialog closed. */
  readonly take: TranscriptId | null;
  readonly onCancel: () => void;
  readonly onConfirm: (id: TranscriptId) => void;
}

export function DeleteTakeDialog({ take, onCancel, onConfirm }: DeleteTakeDialogProps) {
  return (
    <ConfirmDialog
      open={take !== null}
      title="Delete this take?"
      description="Its text and saved audio are removed. This can't be undone."
      confirmLabel="Delete"
      onCancel={onCancel}
      onConfirm={() => {
        if (take !== null) {
          onConfirm(take);
        }
      }}
    />
  );
}
