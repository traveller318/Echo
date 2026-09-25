/**
 * SOURCE OF TRUTH KEYWORDS: TakeRowActions, row actions, copy take, retry take, delete take, DataList actions slot
 * WHAT:  The DataList `actions` slot of a take row: Copy, Retry and Delete for one take. Copy and Retry run at
 *        once; Delete asks first through `onDelete` (the confirmation dialog of TakeOverlays).
 * WHY:   04 §5: row actions on hover or focus. The mutations are shared (useTranscriptActions), so a retry started
 *        here shows as busy on its row and in the detail sheet alike; availability comes from takeAvailability, and
 *        Rust has the final say.
 * WHERE: The DataList actions slot of routes/history and routes/dashboard (recent takes).
 */
import { CopyIcon, RotateCcwIcon, Trash2Icon } from "lucide-react";
import type { TranscriptSummary } from "@/bindings";
import type { TranscriptActions } from "@/hooks";
import { TakeActionButton } from "./TakeActionButton";
import { takeAvailability } from "./take-availability";

export interface TakeRowActionsProps {
  readonly take: TranscriptSummary;
  readonly actions: TranscriptActions;
  readonly onDelete: (take: TranscriptSummary) => void;
}

export function TakeRowActions({ take, actions, onDelete }: TakeRowActionsProps) {
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
