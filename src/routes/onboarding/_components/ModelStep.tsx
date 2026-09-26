/**
 * SOURCE OF TRUTH KEYWORDS: ModelStep, onboarding model step, speech model download, import model from folder, model progress in onboarding, offline mode in onboarding
 * WHAT:  Onboarding step 2: the selected speech engine's Models card with its download (live progress, resume,
 *        cancel) or "Import from folder", a notice while offline mode blocks downloads, and a confirmation once the
 *        model is ready.
 * WHY:   02 §8.2: a missing model is set up here or on the Models page, and both show the same card, actions and live
 *        progress (components/global/model-card, hooks/use-models.ts), so a download started here continues on the
 *        Models page and the reverse. Onboarding offers only import besides the primary action: removing or checking
 *        files belongs to the Models page. Readiness comes from Rust's onboarding view (the flow's Continue waits for
 *        it), refreshed by ModelsChanged when the install finishes.
 * WHERE: onboarding-steps.ts (the `model` entry).
 */
import { CircleCheckIcon, WifiOffIcon } from "lucide-react";
import {
  InlineNotice,
  ModelCard,
  ModelCardActions,
  ProgressBar,
  type SecondaryModelAction,
} from "@/components/global";
import { useModelActions, useModels, useModelTransfers } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";
import type { OnboardingStepProps } from "./step-props";

const OFFLINE = describeAppError({ code: "PermissionDenied", permission: "network" });

const OFFERED: readonly SecondaryModelAction[] = ["import"];

export function ModelStep({ view }: OnboardingStepProps) {
  const models = useModels();
  const transferOf = useModelTransfers();
  const actions = useModelActions();

  if (models.isError) {
    const copy = describeAppError(toAppError(models.error));
    return <InlineNotice title={copy.title} body={copy.body} />;
  }
  if (models.data === undefined) {
    return <ProgressBar value={null} aria-label="Loading the speech model" />;
  }

  const entry = models.data.entries.find((candidate) => candidate.engine.id === view.speech_engine);
  if (entry === undefined) {
    return <InlineNotice title="No speech model to set up" body="This build of Echo lists no speech engine model." />;
  }
  const online = models.data.network === "granted";
  const busy = actions.pending || models.data.entries.some((candidate) => transferOf(candidate) !== null);
  const transfer = transferOf(entry);

  return (
    <div className="flex flex-col gap-4">
      {online ? null : (
        <InlineNotice
          icon={<WifiOffIcon />}
          title={OFFLINE.title}
          body="Downloads are off. Import the model from a folder, or turn offline mode off in Settings later."
        />
      )}
      <ModelCard
        entry={entry}
        transfer={transfer}
        actions={
          <ModelCardActions
            entry={entry}
            transfer={transfer}
            actions={actions}
            online={online}
            busy={busy}
            offer={OFFERED}
          />
        }
      />
      {view.speech_model_ready ? (
        <p role="status" className="flex items-center gap-2 text-footnote text-fg-secondary">
          <CircleCheckIcon aria-hidden="true" className="size-icon-sm shrink-0 text-success" />
          The speech model is installed.
        </p>
      ) : null}
    </div>
  );
}
