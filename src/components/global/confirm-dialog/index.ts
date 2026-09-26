/**
 * SOURCE OF TRUTH KEYWORDS: confirm-dialog barrel, ConfirmDialog export
 * WHAT:  Barrel for the confirm-dialog folder.
 * WHY:   Callers import the folder, never the file, so the component can gain siblings without import churn.
 * WHERE: components/global/index.ts; take-actions/DeleteTakeDialog.
 */
export { ConfirmDialog, type ConfirmDialogProps } from "./ConfirmDialog";
