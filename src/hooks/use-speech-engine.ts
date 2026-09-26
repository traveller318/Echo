/**
 * SOURCE OF TRUTH KEYWORDS: useSpeechEngineStatus, useRemeasureAccelerator, SPEECH_ENGINE_QUERY, SPEECH_ENGINE_EVENTS, engine_status, engine_remeasure, accelerator in use, GPU benchmark
 * WHAT:  The speech engine data layer: `useSpeechEngineStatus()` reads `engine_status` (readiness plus, once ready,
 *        the accelerator in use, why, the GPU and the measured timings); `useRemeasureAccelerator()` runs
 *        `engine_remeasure` (forget the remembered measurement, measure again on automatic) with a calm toast.
 * WHY:   Rust owns where the engine runs (root CLAUDE.md §7): the status refetches on ModelsChanged, which the ASR
 *        worker's readiness relay sends whenever the readiness or the running accelerator changes (including a CPU
 *        fallback), and on SettingsChanged (a new engine or accelerator preference), never by polling.
 * WHERE: Settings → About (step 25) shows the engine and the accelerator in use; any page may read it.
 */
import type { UseQueryResult } from "@tanstack/react-query";
import { commands, type SpeechEngineStatus } from "@/bindings";
import type { EchoEventName } from "@/lib/echo-events";
import { showToast } from "@/stores/toast-store";
import { useEchoMutation } from "./use-echo-mutation";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";

/** Rust events after which the speech engine status is stale. */
export const SPEECH_ENGINE_EVENTS: readonly EchoEventName[] = ["modelsChanged", "settingsChanged"];

export const SPEECH_ENGINE_QUERY: EchoQuery<SpeechEngineStatus> = {
  queryKey: ["engine", "status"],
  command: commands.engineStatus,
  invalidatedBy: SPEECH_ENGINE_EVENTS,
};

export function useSpeechEngineStatus(): UseQueryResult<SpeechEngineStatus> {
  return useEchoQuery(SPEECH_ENGINE_QUERY);
}

export interface RemeasureAccelerator {
  /** Forgets the remembered measurement; on automatic the engine is measured again right away. */
  readonly remeasure: () => void;
  readonly pending: boolean;
}

export function useRemeasureAccelerator(): RemeasureAccelerator {
  const remeasure = useEchoMutation<undefined, null>(commands.engineRemeasure);
  return {
    remeasure: () => {
      remeasure.mutate(undefined, {
        onSuccess: () =>
          showToast({ title: "Measuring again", body: "Echo is checking which processor runs speech fastest." }),
      });
    },
    pending: remeasure.isPending,
  };
}
