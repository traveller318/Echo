/**
 * SOURCE OF TRUTH KEYWORDS: useModels, useModelTransfers, useModelActions, MODELS_QUERY, MODELS_EVENTS, models_list, ModelProgress, live transfer progress, model download actions
 * WHAT:  The Models data layer: `useModels()` reads `models_list` (every engine that runs a model, its status,
 *        selection, runtime and running transfer, plus whether downloads may run); `useModelTransfers()` gives
 *        each model's live transfer (the list's snapshot, then every ModelProgress event); `useModelActions()`
 *        runs download, cancel, import, verify, remove and "use" with calm success toasts.
 * WHY:   Rust owns model state (root CLAUDE.md §7): the list refetches on ModelsChanged (installed, removed, found
 *        damaged, the engine loading) and SettingsChanged (offline mode, the selected engine), never by polling.
 *        Progress arrives at up to 10 Hz, so it is followed from its event instead of refetching the list; the
 *        list's own `transfer` covers a page opened mid-download until the next event. A terminal phase (ready,
 *        cancelled, failed) ends the live entry, and ModelsChanged then brings the settled status. Failures toast
 *        through the AppError copy table (useEchoMutation); a download that was cancelled or an import whose
 *        picker was closed is not a failure.
 * WHERE: routes/models; onboarding's model step (step 24) reuses the same reads and actions.
 */
import type { UseQueryResult } from "@tanstack/react-query";
import { useState } from "react";
import {
  commands,
  type EngineId,
  type ModelEntry,
  type ModelId,
  type ModelPhase,
  type ModelProgress,
  type ModelTransferOutcome,
  type ModelsView,
} from "@/bindings";
import type { EchoEventName } from "@/lib/echo-events";
import { showToast } from "@/stores/toast-store";
import { useEchoEvent } from "./use-echo-event";
import { useEchoMutation } from "./use-echo-mutation";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";

/** Rust events after which the models list is stale. */
export const MODELS_EVENTS: readonly EchoEventName[] = ["modelsChanged", "settingsChanged"];

export const MODELS_QUERY: EchoQuery<ModelsView> = {
  queryKey: ["models", "list"],
  command: commands.modelsList,
  invalidatedBy: MODELS_EVENTS,
};

export function useModels(): UseQueryResult<ModelsView> {
  return useEchoQuery(MODELS_QUERY);
}

const TERMINAL_PHASES: ReadonlySet<ModelPhase> = new Set<ModelPhase>(["ready", "cancelled", "failed"]);

/** Whether a progress report ends its transfer. */
export function isTerminalPhase(phase: ModelPhase): boolean {
  return TERMINAL_PHASES.has(phase);
}

/** The running transfer of each model: the latest report, or null when none runs. */
export type ModelTransfers = (entry: ModelEntry) => ModelProgress | null;

/**
 * SOURCE OF TRUTH KEYWORDS: useModelTransfers, live progress, ModelProgress subscription, transfer snapshot merge
 * WHAT:  Follows ModelProgress events and returns a lookup: a model's latest live report while its transfer runs,
 *        else the list's own snapshot (`entry.transfer`), else null.
 * WHY:   The event is newer than any list read, so it wins; its terminal phase means "nothing runs" even while the
 *        list refetch that follows is still in flight.
 * WHERE: routes/models (each card's progress and actions).
 */
export function useModelTransfers(): ModelTransfers {
  const [live, setLive] = useState<ReadonlyMap<ModelId, ModelProgress>>(() => new Map());
  useEchoEvent("modelProgress", (progress) => {
    setLive((current) => new Map(current).set(progress.model_id, progress));
  });
  return (entry) => {
    const latest = live.get(entry.model.id);
    if (latest === undefined) {
      return entry.transfer;
    }
    return isTerminalPhase(latest.phase) ? null : latest;
  };
}

export interface ModelActions {
  readonly download: (model: ModelManifestRef) => void;
  readonly cancel: (model: ModelManifestRef) => void;
  readonly importFolder: (model: ModelManifestRef) => void;
  readonly verify: (model: ModelManifestRef) => void;
  readonly remove: (model: ModelManifestRef) => void;
  readonly use: (engine: EngineRef) => void;
  /** A download, import, check or removal was asked for and has not answered yet. */
  readonly pending: boolean;
}

/** What an action needs to know about a model: its id, and its label for the toast. */
export interface ModelManifestRef {
  readonly id: ModelId;
  readonly label: string;
}

export interface EngineRef {
  readonly id: EngineId;
  readonly label: string;
}

function transferToast(outcome: ModelTransferOutcome, done: { title: string; body: string }) {
  if (outcome === "completed") {
    showToast(done);
  }
}

export function useModelActions(): ModelActions {
  const download = useEchoMutation(commands.modelsDownload);
  const cancel = useEchoMutation(commands.modelsCancelDownload);
  const importFolder = useEchoMutation(commands.modelsImport);
  const verify = useEchoMutation(commands.modelsVerify);
  const remove = useEchoMutation(commands.modelsRemove);
  const use = useEchoMutation(commands.modelsSetActive);
  return {
    download: (model) => {
      download.mutate(
        { model_id: model.id },
        {
          onSuccess: (outcome) => {
            if (outcome === "cancelled") {
              showToast({ title: "Download paused", body: "It picks up where it stopped when you resume it." });
            }
            transferToast(outcome, { title: "Model installed", body: `${model.label} is ready to use.` });
          },
        },
      );
    },
    cancel: (model) => {
      cancel.mutate({ model_id: model.id });
    },
    importFolder: (model) => {
      importFolder.mutate(
        { model_id: model.id },
        {
          onSuccess: (outcome) => {
            transferToast(outcome, { title: "Model imported", body: `${model.label} is ready to use.` });
          },
        },
      );
    },
    verify: (model) => {
      verify.mutate(
        { model_id: model.id },
        {
          onSuccess: (outcome) => {
            transferToast(outcome, { title: "Model checked", body: `Every file of ${model.label} is intact.` });
          },
        },
      );
    },
    remove: (model) => {
      remove.mutate(
        { model_id: model.id },
        { onSuccess: () => showToast({ title: "Model removed", body: `${model.label} was deleted from this PC.` }) },
      );
    },
    use: (engine) => {
      use.mutate(
        { engine_id: engine.id },
        { onSuccess: () => showToast({ title: "Engine switched", body: `${engine.label} is warming up.` }) },
      );
    },
    pending: download.isPending || importFolder.isPending || verify.isPending || remove.isPending,
  };
}
