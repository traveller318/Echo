/**
 * SOURCE OF TRUTH KEYWORDS: setupNoticeCopy, model setup notice copy, selected engine not ready, download progress line
 * WHAT:  `setupNoticeCopy(entry, transfer)`: the line a selected engine's setup notice shows (the running
 *        transfer's progress, or what to do about a model that is paused, damaged or missing), or null when the
 *        engine is not selected or its model is installed.
 * WHY:   A pure rule, tested as a table, so the notice component only draws it; the progress line is the model
 *        card's own (transferSummary), so both pages read alike.
 * WHERE: ModelSetupNotices (this folder).
 */
import type { ModelEntry, ModelProgress, ModelStatus } from "@/bindings";
import { transferSummary } from "@/components/global";

const STATUS_BODY: Readonly<Record<Exclude<ModelStatus["kind"], "installed">, string>> = {
  not_installed: "Download it on the Models page to use it.",
  partial: "The download stopped. Resume it on the Models page.",
  corrupt: "Its files are damaged. Download it again on the Models page.",
};

export function setupNoticeCopy(entry: ModelEntry, transfer: ModelProgress | null): string | null {
  if (entry.selection.kind !== "selectable" || !entry.selection.active) {
    return null;
  }
  if (transfer !== null) {
    return transferSummary(transfer);
  }
  return entry.status.kind === "installed" ? null : STATUS_BODY[entry.status.kind];
}
