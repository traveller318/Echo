/**
 * SOURCE OF TRUTH KEYWORDS: ModelActions, model card buttons, download button, cancel download, import from folder button, check files, remove model button, use engine
 * WHAT:  The buttons of one model card, drawn from `modelActionPlan`: the primary action (primary style, or plain
 *        for Cancel) and the secondary ones (Import from folder, Check files, Remove).
 * WHY:   The plan decides what is offered; this only labels and wires it. Network actions are disabled while
 *        offline mode is on (the page's notice says why, and the factory would refuse them anyway); everything
 *        but Cancel is disabled while another transfer or removal is waiting for its answer, because Rust would
 *        answer `Busy`. Remove only asks: the page owns the confirmation dialog.
 * WHERE: routes/models/index.tsx, in each ModelCard's `actions` slot.
 */
import type { ModelEntry, ModelProgress } from "@/bindings";
import { Button } from "@/components/ui";
import type { ModelActions as ModelActionRunner } from "@/hooks";
import {
  modelActionPlan,
  needsNetwork,
  type PrimaryModelAction,
  type SecondaryModelAction,
} from "./model-actions";

export interface ModelActionsProps {
  readonly entry: ModelEntry;
  readonly transfer: ModelProgress | null;
  readonly actions: ModelActionRunner;
  /** Downloads are allowed now (offline mode is off). */
  readonly online: boolean;
  /** Another model action is waiting for its answer. */
  readonly busy: boolean;
  readonly onRemove: (entry: ModelEntry) => void;
}

const PRIMARY_LABEL: Readonly<Record<PrimaryModelAction, string>> = {
  download: "Download",
  resume: "Resume download",
  redownload: "Download again",
  cancel: "Cancel",
  use: "Use",
};

const SECONDARY_LABEL: Readonly<Record<SecondaryModelAction, string>> = {
  import: "Import from folder",
  verify: "Check files",
  remove: "Remove",
};

export function ModelActions({ entry, transfer, actions, online, busy, onRemove }: ModelActionsProps) {
  const plan = modelActionPlan(entry, transfer);
  const model = { id: entry.model.id, label: entry.model.label };

  const runPrimary = (action: PrimaryModelAction) => {
    switch (action) {
      case "download":
      case "resume":
      case "redownload":
        actions.download(model);
        return;
      case "cancel":
        actions.cancel(model);
        return;
      case "use":
        actions.use({ id: entry.engine.id, label: entry.engine.label });
    }
  };

  const runSecondary = (action: SecondaryModelAction) => {
    switch (action) {
      case "import":
        actions.importFolder(model);
        return;
      case "verify":
        actions.verify(model);
        return;
      case "remove":
        onRemove(entry);
    }
  };

  return (
    <>
      {plan.secondary.map((action) => (
        <Button
          key={action}
          variant="ghost"
          disabled={busy}
          onClick={() => {
            runSecondary(action);
          }}
        >
          {SECONDARY_LABEL[action]}
        </Button>
      ))}
      {plan.primary === null ? null : (
        <Button
          variant={plan.primary === "cancel" ? "secondary" : "primary"}
          disabled={plan.primary !== "cancel" && (busy || (needsNetwork(plan.primary) && !online))}
          onClick={() => {
            if (plan.primary !== null) {
              runPrimary(plan.primary);
            }
          }}
        >
          {PRIMARY_LABEL[plan.primary]}
        </Button>
      )}
    </>
  );
}
