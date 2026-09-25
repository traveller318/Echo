/**
 * SOURCE OF TRUTH KEYWORDS: TakeOverlays, take detail sheet, delete take confirmation, take inspector overlays
 * WHAT:  The overlays a list of takes opens, driven by a TakeInspector: the detail sheet of the open take and the
 *        confirmation before a delete.
 * WHY:   One element per page renders both, wired the same way wherever takes are listed, so the sheet's actions
 *        and the confirmation cannot drift between History and the Dashboard.
 * WHERE: routes/history, routes/dashboard (next to their DataList). Exported through components/global/index.ts.
 */
import { DeleteTakeDialog } from "./DeleteTakeDialog";
import { TranscriptSheet } from "./TranscriptSheet";
import type { TakeInspector } from "./use-take-inspector";

export interface TakeOverlaysProps {
  readonly inspector: TakeInspector;
}

export function TakeOverlays({ inspector }: TakeOverlaysProps) {
  return (
    <>
      <TranscriptSheet
        id={inspector.openId}
        onClose={inspector.close}
        actions={inspector.actions}
        onDelete={inspector.askDelete}
      />
      <DeleteTakeDialog
        take={inspector.deletingId}
        onCancel={inspector.cancelDelete}
        onConfirm={inspector.confirmDelete}
      />
    </>
  );
}
