/**
 * SOURCE OF TRUTH KEYWORDS: ModelCard, model card, engine card, model status badge, download progress bar, model facts, license, languages, actions slot
 * WHAT:  One engine that runs a model, as a glass card: name and kind, status badge, facts (download size, license,
 *        languages, what it runs on, what it comes with), the credit lines of the model and what it requires, the
 *        running transfer's bar and summary, why it could not load, and an `actions` slot for the buttons.
 * WHY:   04 §5 "one card per engine from the registry": everything comes from the ModelEntry Rust built, so a new
 *        engine is a registry entry with no change here. The card owns presentation only; which actions a page
 *        offers (Models page vs. onboarding's single "Download") come in through the slot. The bar is determinate
 *        while bytes move and indeterminate while installing; numbers are tabular (04 §3.6). A load failure shows
 *        the AppError copy of the error Rust reported, the same words a toast would use. The size is what a download
 *        moves for the whole card (the model and its runtime, 02 §8.2), computed in Rust, so the card never adds
 *        manifests itself.
 * WHERE: routes/models (every entry); onboarding's model step (step 24). Exported through components/global.
 */
import { useId, type ReactNode } from "react";
import type { ModelEntry, ModelProgress } from "@/bindings";
import { Badge } from "@/components/ui";
import { describeAppError } from "@/lib/app-error";
import { cn } from "@/lib/cn";
import { formatBytes, NUMERIC_CLASS } from "@/lib/format";
import { FactList, type Fact } from "../fact-list";
import { GlassSurface } from "../glass-surface";
import { ProgressBar } from "../progress-bar";
import {
  acceleratorLabel,
  engineKindLabel,
  isDeterminate,
  languagesSummary,
  modelStatusLook,
  transferSummary,
} from "./model-look";

export interface ModelCardProps {
  readonly entry: ModelEntry;
  /** The transfer running for this model now (live), or null. */
  readonly transfer: ModelProgress | null;
  /** Buttons for this model. */
  readonly actions?: ReactNode;
  readonly className?: string;
}

export function ModelCard({ entry, transfer, actions, className }: ModelCardProps) {
  const headingId = useId();
  const look = modelStatusLook(entry, transfer);
  const languages = languagesSummary(entry.engine.caps);
  const failure = entry.runtime?.kind === "failed" ? describeAppError(entry.runtime.error) : null;
  const runsOn = entry.runtime?.kind === "ready" && entry.runtime.accelerator !== null ? entry.runtime.accelerator : null;
  const credits = [entry.model, ...entry.requires].flatMap((manifest) =>
    manifest.attribution === null ? [] : [manifest.attribution],
  );
  const facts: Fact[] = [
    { label: "Size", value: formatBytes(entry.download_bytes), numeric: true },
    { label: "License", value: entry.model.license },
    ...(languages === null ? [] : [{ label: "Languages", value: languages }]),
    ...(runsOn === null ? [] : [{ label: "Runs on", value: acceleratorLabel(runsOn) }]),
    ...(entry.requires.length === 0
      ? []
      : [{ label: "Includes", value: entry.requires.map((required) => required.label).join(", ") }]),
  ];
  const Icon = look.icon;
  return (
    <GlassSurface asChild className={cn("flex flex-col gap-4 p-5", className)}>
      <article aria-labelledby={headingId} data-slot="model-card">
        <header className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 flex-col gap-1">
            <h2 id={headingId} className="text-title3 text-fg">
              {entry.engine.label}
            </h2>
            <p className="text-footnote text-fg-secondary">{engineKindLabel(entry.engine.caps.kind)}</p>
          </div>
          <Badge variant={look.variant}>
            <Icon aria-hidden="true" />
            {look.label}
          </Badge>
        </header>
        <FactList facts={facts} />
        {credits.map((credit) => (
          <p key={credit} className="text-caption text-fg-secondary">
            {credit}
          </p>
        ))}
        {transfer === null ? null : (
          <div className="flex flex-col gap-2" data-slot="model-card-transfer">
            <ProgressBar
              value={isDeterminate(transfer) ? transfer.bytes : null}
              max={transfer.total}
              aria-label={`${entry.engine.label}: ${look.label}`}
            />
            <p className={cn("text-footnote text-fg-secondary", NUMERIC_CLASS)} aria-live="polite">
              {transferSummary(transfer)}
            </p>
          </div>
        )}
        {failure === null ? null : (
          <p className="text-footnote text-fg-secondary" role="status">
            {failure.title}. {failure.body}
          </p>
        )}
        {actions === undefined ? null : (
          <footer className="flex flex-wrap items-center justify-end gap-2" data-slot="model-card-actions">
            {actions}
          </footer>
        )}
      </article>
    </GlassSurface>
  );
}

