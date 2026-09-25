/**
 * SOURCE OF TRUTH KEYWORDS: modelActionPlan, primary model action, download resume redownload cancel use, model card buttons, secondary model actions
 * WHAT:  `modelActionPlan(entry, transfer)`: which button a model card leads with (download, resume, download
 *        again, cancel, use, or none) and which secondary actions it offers (import from a folder, check files,
 *        remove).
 * WHY:   04 §5: one primary action per card (Download / Use / Remove family) and "Import from folder" beside it. The
 *        choice is a pure function of what Rust reported, so it is tested as a table and the component only draws
 *        it. A running transfer offers only Cancel (every other action would be `Busy`); a bundled model offers
 *        nothing (it ships with Echo); an engine in use is not offered "Use". Removing is offered for anything that
 *        takes disk space (installed, damaged, or a partial download).
 * WHERE: ModelActions (this folder).
 */
import type { ModelEntry, ModelProgress } from "@/bindings";

export type PrimaryModelAction = "download" | "resume" | "redownload" | "cancel" | "use";
export type SecondaryModelAction = "import" | "verify" | "remove";

export interface ModelActionPlan {
  readonly primary: PrimaryModelAction | null;
  readonly secondary: readonly SecondaryModelAction[];
}

const NOTHING: ModelActionPlan = { primary: null, secondary: [] };

export function modelActionPlan(entry: ModelEntry, transfer: ModelProgress | null): ModelActionPlan {
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

/** Whether an action needs the network (offline mode turns it off). */
export function needsNetwork(action: PrimaryModelAction): boolean {
  return action === "download" || action === "resume" || action === "redownload";
}
