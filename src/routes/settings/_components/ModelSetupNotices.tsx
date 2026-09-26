/**
 * SOURCE OF TRUTH KEYWORDS: ModelSetupNotices, selected model not installed notice, grammar polish download progress, settings model notice, model setup
 * WHAT:  One InlineNotice per engine the settings select whose model is not ready: downloading (live progress),
 *        paused, damaged or not installed, with a button to the Models page. Renders nothing when every selected
 *        engine is installed.
 * WHY:   Switching grammar polish on starts a 1.3 GB download in the background (02 §8.2); the page where the switch
 *        was flipped must say so and show how far it is, or the feature would look broken until it finishes. It is
 *        generic over the models list (an engine selected by any setting), so a future selectable engine needs no
 *        change here. The badge words and the progress line come from the model card's copy helpers, so Settings
 *        and the Models page describe a model the same way (model-setup-copy.ts); progress follows ModelProgress
 *        events, never polling.
 * WHERE: routes/settings/index.tsx, above the sections.
 */
import { useOpenPage } from "@/app/shell/use-open-page";
import { InlineNotice, modelStatusLook } from "@/components/global";
import { Button } from "@/components/ui";
import { useModels, useModelTransfers } from "@/hooks";
import { setupNoticeCopy } from "./model-setup-copy";

export function ModelSetupNotices() {
  const models = useModels();
  const transferOf = useModelTransfers();
  const openPage = useOpenPage();
  if (models.data === undefined) {
    return null;
  }
  return models.data.entries.map((entry) => {
    const transfer = transferOf(entry);
    const body = setupNoticeCopy(entry, transfer);
    if (body === null) {
      return null;
    }
    const look = modelStatusLook(entry, transfer);
    const Icon = look.icon;
    return (
      <InlineNotice
        key={entry.engine.id}
        icon={<Icon />}
        title={`${entry.engine.label}: ${look.label}`}
        body={body}
        action={
          <Button
            size="sm"
            onClick={() => {
              openPage("models");
            }}
          >
            Open Models
          </Button>
        }
      />
    );
  });
}
