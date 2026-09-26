/**
 * SOURCE OF TRUTH KEYWORDS: ConfirmDialog, destructive confirmation, confirm before delete, cancel focused first, are you sure dialog
 * WHAT:  A modal that asks before an action that cannot be undone: a title, a description, Cancel and one
 *        destructive confirm button. `open` false keeps it closed; Cancel, Esc or the overlay call `onCancel`.
 * WHY:   Deleting a take, removing a model and clearing History all confirm the same way (04 §1 "red only means …
 *        destructive"): the confirm button in --color-record, Cancel first so Radix focuses it and Enter never
 *        confirms by accident. The copy comes in as props, so each caller keeps its own words.
 * WHERE: take-actions/DeleteTakeDialog, routes/models RemoveModelDialog, routes/history (clear History).
 *        Exported through components/global/index.ts.
 */
import type { ReactNode } from "react";
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

export interface ConfirmDialogProps {
  readonly open: boolean;
  readonly title: ReactNode;
  readonly description: ReactNode;
  /** The destructive button's label (e.g. "Delete"). */
  readonly confirmLabel: string;
  readonly onCancel: () => void;
  readonly onConfirm: () => void;
}

export function ConfirmDialog({ open, title, description, confirmLabel, onCancel, onConfirm }: ConfirmDialogProps) {
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          onCancel();
        }
      }}
    >
      <DialogContent showClose={false}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose asChild>
            <Button>Cancel</Button>
          </DialogClose>
          <Button variant="destructive" onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
