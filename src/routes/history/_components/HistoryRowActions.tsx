/**
 * SOURCE OF TRUTH KEYWORDS: HistoryRowActions, row actions, copy take, retry take, delete take, DataList actions slot
 * WHAT:  The DataList `actions` slot of a History row: Copy, Retry and Delete for one take. Copy and Retry run at
 *        once; Delete asks first through `onDelete` (the page's confirmation dialog).
 * WHY:   04 §5: row actions on hover or focus. The mutations are shared (useTranscriptActions), so a retry started
 *        here shows as busy on its row and in the detail sheet alike; availability comes from takeAvailability, and
 *        Rust has the final say.
 * WHERE: routes/history/index.tsx (DataList actions slot).
 */
import { CopyIcon, RotateCcwIcon, Trash2Icon } from "lucide-react";
import type { TranscriptSummary } from "@/bindings";
import type { TranscriptActions } from "@/hooks";
import { TakeActionButton } from "./TakeActionButton";
import { takeAvailability } from "./take-availability";

export interface HistoryRowActionsProps {
  readonly take: TranscriptSummary;
  readonly actions: TranscriptActions;
  readonly onDelete: (take: TranscriptSummary) => void;
}

export function HistoryRowActions({ take, actions, onDelete }: HistoryRowActionsProps) {
  const can = takeAvailability(take);
  const retrying = actions.retry.isPending && actions.retry.variables.id === take.id;
  return (
    <>
      <TakeActionButton
        label="Copy"
        icon={CopyIcon}
        disabled={!can.copy}
        onClick={() => {
          actions.copy.run(take.id);
        }}
      />
      <TakeActionButton
        label="Retry"
        busyLabel="Retrying"
        icon={RotateCcwIcon}
        disabled={!can.retry}
        busy={retrying}
        onClick={() => {
          actions.retry.run(take.id);
        }}
      />
      <TakeActionButton
        label="Delete"
        icon={Trash2Icon}
        disabled={!can.remove}
        onClick={() => {
          onDelete(take);
        }}
      />
    </>
  );
}
