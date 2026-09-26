/**
 * SOURCE OF TRUTH KEYWORDS: ModelsPage, models route, models page, model cards, offline mode notice, import from folder, remove model confirmation
 * WHAT:  The Models screen (04 §5): one ModelCard per engine that runs a model, each with its actions (download,
 *        resume, cancel, use, import from folder, check files, remove), a notice while offline mode blocks
 *        downloads, and the confirmation before a model is removed.
 * WHY:   Everything comes from `models_list` and ModelProgress (hooks/use-models.ts); the page keeps no copy of
 *        model state, so a download started here, in onboarding or from the pill's "Set up" shows the same way.
 *        The offline notice uses the network permission's AppError copy and action (lib/app-error.ts), so its words
 *        match the error a refused download would give and its button opens Settings without this page naming a
 *        setting. While any transfer runs or an action waits for its answer, the other actions are disabled
 *        (Rust would answer Busy); Cancel always works. Loading shows an indeterminate bar after --delay-loading.
 * WHERE: Lazy-loaded by app/routes.tsx for the `models` nav entry (app/nav-page.ts).
 */
import { WifiOffIcon } from "lucide-react";
import { useState } from "react";
import type { ModelEntry } from "@/bindings";
import type { NavPageProps } from "@/app/nav-page";
import { useAppErrorAction } from "@/app/shell/use-app-error-action";
import { EmptyState, InlineNotice, ModelCard, ModelCardActions, NavIcon, Page, ProgressBar } from "@/components/global";
import { Button } from "@/components/ui";
import { useModelActions, useModels, useModelTransfers } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";
import { RemoveModelDialog } from "./_components/RemoveModelDialog";

const OFFLINE_COPY = describeAppError({ code: "PermissionDenied", permission: "network" });

export default function ModelsPage({ nav }: NavPageProps) {
  const models = useModels();
  const transferOf = useModelTransfers();
  const actions = useModelActions();
  const performAction = useAppErrorAction();
  const [removing, setRemoving] = useState<ModelEntry | null>(null);

  if (models.isError) {
    const copy = describeAppError(toAppError(models.error));
    return (
      <Page title={nav.label}>
        <EmptyState
          icon={<NavIcon icon={nav.icon} />}
          title={copy.title}
          body={copy.body}
          action={
            <Button
              onClick={() => {
                void models.refetch();
              }}
            >
              Try again
            </Button>
          }
        />
      </Page>
    );
  }

  if (models.data === undefined) {
    return (
      <Page title={nav.label}>
        <ProgressBar value={null} aria-label="Loading models" />
      </Page>
    );
  }

  const view = models.data;
  const online = view.network === "granted";
  const busy = actions.pending || view.entries.some((entry) => transferOf(entry) !== null);
  const offlineAction = OFFLINE_COPY.action;
  return (
    <Page title={nav.label}>
      {online ? null : (
        <InlineNotice
          icon={<WifiOffIcon />}
          title={OFFLINE_COPY.title}
          body="Downloads are off. You can still import a model from a folder."
          action={
            offlineAction === null ? undefined : (
              <Button
                size="sm"
                onClick={() => {
                  performAction(offlineAction.id);
                }}
              >
                {offlineAction.label}
              </Button>
            )
          }
        />
      )}
      {view.entries.length === 0 ? (
        <EmptyState icon={<NavIcon icon={nav.icon} />} title="No models" body="This build of Echo lists no models." />
      ) : (
        view.entries.map((entry) => {
          const transfer = transferOf(entry);
          return (
            <ModelCard
              key={entry.engine.id}
              entry={entry}
              transfer={transfer}
              actions={
                <ModelCardActions
                  entry={entry}
                  transfer={transfer}
                  actions={actions}
                  online={online}
                  busy={busy}
                  onRemove={setRemoving}
                />
              }
            />
          );
        })
      )}
      <RemoveModelDialog
        entry={removing}
        onCancel={() => {
          setRemoving(null);
        }}
        onConfirm={(entry) => {
          setRemoving(null);
          actions.remove({ id: entry.model.id, label: entry.model.label });
        }}
      />
    </Page>
  );
}
