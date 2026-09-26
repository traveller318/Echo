/**
 * SOURCE OF TRUTH KEYWORDS: modelStatusLook, model status badge, transferSummary, engineKindLabel, languagesSummary, acceleratorLabel, model card copy, ModelStatus label
 * WHAT:  How a model card reads: its status badge (label, variant, glyph) from the status, selection, runtime and
 *        running transfer; a one-line transfer summary ("312 MB of 670 MB"); the engine kind's name; a short
 *        language list ("English, German, French and 22 more"); and what an accelerator is called.
 * WHY:   Every surface that shows a model (Models page, onboarding's model step) must say the same thing, so the
 *        mapping lives once here, keyed by the generated unions (a new status, phase or kind fails tsc until it has
 *        copy). A running transfer wins over the stored status (it is what is happening now); an engine that is in
 *        use shows its runtime (loading, or could not load) because that is what a take would get. Status is never
 *        colour alone (04 §7): every look has a glyph and a word. Copy is calm (04 §1).
 * WHERE: ModelCard (this folder); routes/models.
 */
import {
  CircleAlertIcon,
  CircleCheckIcon,
  CircleDashedIcon,
  CloudDownloadIcon,
  LoaderIcon,
  PackageCheckIcon,
  PauseIcon,
  WifiOffIcon,
  type LucideIcon,
} from "lucide-react";
import type { Accelerator, EngineCaps, EngineKind, ModelEntry, ModelPhase, ModelProgress } from "@/bindings";
import { formatBytes, formatLanguage } from "@/lib/format";
import type { BadgeVariant } from "../transcript-row";

const ACCELERATOR_LABEL: Readonly<Record<Accelerator, string>> = {
  cpu: "Processor (CPU)",
  gpu: "Graphics card (GPU)",
};

/** What `accelerator` is called wherever Echo says where a model runs. */
export function acceleratorLabel(accelerator: Accelerator): string {
  return ACCELERATOR_LABEL[accelerator];
}

export interface ModelStatusLook {
  readonly label: string;
  readonly variant: BadgeVariant;
  readonly icon: LucideIcon;
}

const PHASE_LOOK: Readonly<Record<ModelPhase, ModelStatusLook>> = {
  transferring: { label: "Downloading", variant: "accent", icon: CloudDownloadIcon },
  waiting: { label: "Waiting for connection", variant: "warning", icon: WifiOffIcon },
  verifying: { label: "Checking", variant: "accent", icon: LoaderIcon },
  installing: { label: "Installing", variant: "accent", icon: LoaderIcon },
  ready: { label: "Installed", variant: "success", icon: CircleCheckIcon },
  cancelled: { label: "Paused", variant: "neutral", icon: PauseIcon },
  failed: { label: "Didn't finish", variant: "warning", icon: CircleAlertIcon },
};

const LOOKS = {
  builtIn: { label: "Built in", variant: "neutral", icon: PackageCheckIcon },
  inUse: { label: "In use", variant: "accent", icon: CircleCheckIcon },
  loading: { label: "Loading", variant: "accent", icon: LoaderIcon },
  loadFailed: { label: "Couldn't load", variant: "warning", icon: CircleAlertIcon },
  installed: { label: "Installed", variant: "success", icon: CircleCheckIcon },
  partial: { label: "Paused", variant: "neutral", icon: PauseIcon },
  corrupt: { label: "Damaged", variant: "warning", icon: CircleAlertIcon },
  notInstalled: { label: "Not installed", variant: "neutral", icon: CircleDashedIcon },
} as const satisfies Readonly<Record<string, ModelStatusLook>>;

/** The badge a model card shows now. */
export function modelStatusLook(entry: ModelEntry, transfer: ModelProgress | null): ModelStatusLook {
  if (transfer !== null) {
    return PHASE_LOOK[transfer.phase];
  }
  switch (entry.status.kind) {
    case "not_installed":
      return LOOKS.notInstalled;
    case "partial":
      return LOOKS.partial;
    case "corrupt":
      return LOOKS.corrupt;
    case "installed":
      return installedLook(entry);
  }
}

function installedLook(entry: ModelEntry): ModelStatusLook {
  if (entry.selection.kind === "built_in") {
    return LOOKS.builtIn;
  }
  if (!entry.selection.active) {
    return LOOKS.installed;
  }
  switch (entry.runtime?.kind) {
    case "loading":
      return LOOKS.loading;
    case "failed":
      return LOOKS.loadFailed;
    case "ready":
    case undefined:
      return LOOKS.inUse;
  }
}

/**
 * SOURCE OF TRUTH KEYWORDS: transferSummary, download progress text, bytes of total, waiting for connection copy
 * WHAT:  One line under a running transfer's bar: bytes of total while bytes move (also while waiting to resume),
 *        and what is happening once they have all arrived.
 * WHY:   The bar shows how far; the line says what, so "checking" never looks like a stalled download.
 * WHERE: ModelCard.
 */
export function transferSummary(progress: ModelProgress, locale?: string): string {
  const moved = `${formatBytes(progress.bytes, locale)} of ${formatBytes(progress.total, locale)}`;
  switch (progress.phase) {
    case "transferring":
      return moved;
    case "waiting":
      return `${moved}. The connection dropped; resuming shortly.`;
    case "verifying":
      return "Checking every file";
    case "installing":
      return "Installing";
    case "ready":
      return "Installed";
    case "cancelled":
      return `Paused at ${moved}`;
    case "failed":
      return `Stopped at ${moved}`;
  }
}

/** Whether a transfer's bar can show a fraction (bytes against a known total). */
export function isDeterminate(progress: ModelProgress): boolean {
  return progress.total > 0 && progress.phase !== "installing";
}

const ENGINE_KIND_LABEL: Readonly<Record<EngineKind, string>> = {
  asr: "Speech recognition",
  vad: "Voice detection",
  polisher: "Text cleanup",
};

export function engineKindLabel(kind: EngineKind): string {
  return ENGINE_KIND_LABEL[kind];
}

/** How many language names a summary spells out before "and N more". */
const NAMED_LANGUAGES = 3;

/**
 * SOURCE OF TRUTH KEYWORDS: languagesSummary, engine languages, supported languages text, any language
 * WHAT:  The languages an engine's caps declare, as a short phrase; null when the engine has no language (a voice
 *        detector).
 * WHY:   05 A14: the Models page shows each engine's languages from its caps, so users see what it understands;
 *        a 25-language list would swamp a card, so it names a few and counts the rest.
 * WHERE: ModelCard.
 */
export function languagesSummary(caps: EngineCaps, locale?: string): string | null {
  const codes = languageCodes(caps);
  if (codes === null) {
    return caps.kind === "polisher" ? "Any language" : null;
  }
  const names = codes.slice(0, NAMED_LANGUAGES).map((code) => formatLanguage(code, locale));
  const rest = codes.length - names.length;
  return rest > 0 ? `${names.join(", ")} and ${String(rest)} more` : names.join(", ");
}

function languageCodes(caps: EngineCaps): readonly string[] | null {
  switch (caps.kind) {
    case "asr":
      return caps.languages;
    case "polisher":
      return caps.languages.kind === "only" ? caps.languages.languages : null;
    case "vad":
      return null;
  }
}
