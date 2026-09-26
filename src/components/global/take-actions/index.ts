/**
 * SOURCE OF TRUTH KEYWORDS: take-actions barrel, TakeRowActions, TakeOverlays, useTakeInspector, DeleteTakeDialog, takeAvailability
 * WHAT:  Barrel for the take-actions folder: everything a list of takes offers on its rows (Copy, Retry, Delete),
 *        the delete confirmation and the state that ties them together.
 * WHY:   Callers import the folder, never the file, so the parts can be reorganised without import churn.
 * WHERE: components/global/index.ts.
 */
export { DeleteTakeDialog, type DeleteTakeDialogProps } from "./DeleteTakeDialog";
export { TakeActionButton, type TakeActionButtonProps } from "./TakeActionButton";
export { takeAvailability, type TakeAvailability } from "./take-availability";
export { TakeOverlays, type TakeOverlaysProps } from "./TakeOverlays";
export { TakeRowActions, type TakeRowActionsProps } from "./TakeRowActions";
export { useTakeInspector, type TakeInspector } from "./use-take-inspector";
