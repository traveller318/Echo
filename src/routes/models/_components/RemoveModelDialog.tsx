/**
 * SOURCE OF TRUTH KEYWORDS: RemoveModelDialog, confirm remove model, delete model files, destructive confirmation
 * WHAT:  Asks before a model's files are deleted; says so plainly when the engine is the one dictation uses (takes
 *        stop working until it is downloaded or imported again). Remove runs the action and closes; Cancel (or Esc)
 *        keeps it.
 * WHY:   Removing a 670 MB model costs a long download to undo, so it is confirmed like deleting a take
 *        (DeleteTakeDialog): the destructive button in --color-record, Cancel focused first so Enter never removes.
 * WHERE: routes/models/index.tsx.
 */
import type { ModelEntry } from "@/bindings";
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

export interface RemoveModelDialogProps {
  /** The model to remove; null keeps the dialog closed. */
  readonly entry: ModelEntry | null;
  readonly onCancel: () => void;
  readonly onConfirm: (entry: ModelEntry) => void;
}

export function RemoveModelDialog({ entry, onCancel, onConfirm }: RemoveModelDialogProps) {
  const inUse = entry?.selection.kind === "selectable" && entry.selection.active;
  return (
    <Dialog
      open={entry !== null}
      onOpenChange={(open) => {
        if (!open) {
          onCancel();
        }
      }}
    >
      <DialogContent showClose={false}>
        <DialogHeader>
          <DialogTitle>Remove {entry?.engine.label ?? "this model"}?</DialogTitle>
          <DialogDescription>
            {inUse
              ? "Dictation uses this model. It stops working until you download or import the model again."
              : "Its files are deleted from this PC. You can download it again at any time."}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose asChild>
            <Button>Cancel</Button>
          </DialogClose>
          <Button
            variant="destructive"
            onClick={() => {
              if (entry !== null) {
                onConfirm(entry);
              }
            }}
          >
            Remove
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
