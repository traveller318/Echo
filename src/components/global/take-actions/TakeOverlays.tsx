/**
 * SOURCE OF TRUTH KEYWORDS: TakeOverlays, delete take confirmation, take inspector overlays
 * WHAT:  The overlays a list of takes opens, driven by a TakeInspector: the confirmation before a delete.
 * WHY:   One element per page, wired the same way wherever takes are listed, so the confirmation cannot drift
 *        between History and the Dashboard. There is no detail drawer: a row already shows what a take needs and
 *        its actions sit on the row (decision 2026-09-26), so a row click opens nothing.
 * WHERE: routes/history, routes/dashboard (next to their DataList). Exported through components/global/index.ts.
 */
import { DeleteTakeDialog } from "./DeleteTakeDialog";
import type { TakeInspector } from "./use-take-inspector";

export interface TakeOverlaysProps {
  readonly inspector: TakeInspector;
}

export function TakeOverlays({ inspector }: TakeOverlaysProps) {
  return (
    <DeleteTakeDialog
      take={inspector.deletingId}
      onCancel={inspector.cancelDelete}
      onConfirm={inspector.confirmDelete}
    />
  );
}
