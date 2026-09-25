/**
 * SOURCE OF TRUTH KEYWORDS: DeleteTakeDialog, confirm delete take, destructive confirmation, delete audio and text
 * WHAT:  Asks before a take is deleted: its text and saved audio go for good. Delete runs the mutation and closes;
 *        Cancel (or Esc) keeps the take.
 * WHY:   00 constraint 5 "never lose a take": deleting is the one History action that cannot be undone, so it is
 *        confirmed, with the destructive action in --color-record (04 §1 "red only means … destructive") and Cancel
 *        focused first by Radix so Enter never deletes by accident.
 * WHERE: routes/history/index.tsx (from a row's Delete or the detail sheet).
 */
import type { TranscriptId } from "@/bindings";
import {
  Button,
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui";

export interface DeleteTakeDialogProps {
  /** The take to delete; null keeps the dialog closed. */
  readonly take: TranscriptId | null;
  readonly onCancel: () => void;
  readonly onConfirm: (id: TranscriptId) => void;
}

export function DeleteTakeDialog({ take, onCancel, onConfirm }: DeleteTakeDialogProps) {
  return (
    <Dialog
      open={take !== null}
      onOpenChange={(open) => {
        if (!open) {
          onCancel();
        }
      }}
    >
      <DialogContent showClose={false}>
        <DialogHeader>
          <DialogTitle>Delete this take?</DialogTitle>
          <DialogDescription>Its text and saved audio are removed. This can&apos;t be undone.</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose asChild>
            <Button>Cancel</Button>
          </DialogClose>
          <Button
            variant="destructive"
            onClick={() => {
              if (take !== null) {
                onConfirm(take);
              }
            }}
          >
            Delete
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
