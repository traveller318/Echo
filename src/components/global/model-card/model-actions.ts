/**
 * SOURCE OF TRUTH KEYWORDS: modelActionPlan, primary model action, download resume redownload cancel use, model card buttons, secondary model actions, offered actions
 * WHAT:  `modelActionPlan(entry, transfer, offered)`: which button a model card leads with (download, resume,
 *        download again, cancel, use, or none) and which secondary actions it offers (import from a folder, check
 *        files, remove), limited to the secondary actions the surface offers.
 * WHY:   04 §5: one primary action per card (Download / Use / Remove family) and "Import from folder" beside it. The
 *        choice is a pure function of what Rust reported, so it is tested as a table and the component only draws
 *        it. A running transfer offers only Cancel (every other action would be `Busy`); a bundled model offers
 *        nothing (it ships with Echo); an engine in use is not offered "Use". Removing is offered for anything that
 *        takes disk space (installed, damaged, or a partial download). Surfaces differ only in which secondary actions
 *        they offer (onboarding offers import, never removal), so that is a parameter, not a second plan.
 * WHERE: ModelCardActions (this folder), used by routes/models and onboarding's model step.
 */
import type { ModelEntry, ModelProgress } from "@/bindings";

export type PrimaryModelAction = "download" | "resume" | "redownload" | "cancel" | "use";
export type SecondaryModelAction = "import" | "verify" | "remove";

export interface ModelActionPlan {
  readonly primary: PrimaryModelAction | null;
  readonly secondary: readonly SecondaryModelAction[];
}

/** Every secondary action, in the order a card shows them. */
export const ALL_SECONDARY_MODEL_ACTIONS: readonly SecondaryModelAction[] = ["import", "verify", "remove"];

const NOTHING: ModelActionPlan = { primary: null, secondary: [] };

function fullPlan(entry: ModelEntry, transfer: ModelProgress | null): ModelActionPlan {
  if (transfer !== null) {
    return { primary: "cancel", secondary: [] };
  }
  if (entry.model.bundled) {
    return NOTHING;
  }
  switch (entry.status.kind) {
    case "not_installed":
      return { primary: "download", secondary: ["import"] };
    case "partial":
      return { primary: "resume", secondary: ["import", "remove"] };
    case "corrupt":
      return { primary: "redownload", secondary: ["import", "remove"] };
    case "installed":
      return {
        primary: entry.selection.kind === "selectable" && !entry.selection.active ? "use" : null,
        secondary: ["verify", "remove"],
      };
  }
}

export function modelActionPlan(
  entry: ModelEntry,
  transfer: ModelProgress | null,
  offered: readonly SecondaryModelAction[] = ALL_SECONDARY_MODEL_ACTIONS,
): ModelActionPlan {
  const plan = fullPlan(entry, transfer);
  return { primary: plan.primary, secondary: plan.secondary.filter((action) => offered.includes(action)) };
}

/** Whether an action needs the network (offline mode turns it off). */
export function needsNetwork(action: PrimaryModelAction): boolean {
  return action === "download" || action === "resume" || action === "redownload";
}
