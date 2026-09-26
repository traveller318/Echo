/**
 * SOURCE OF TRUTH KEYWORDS: about copy, engineSummary, acceleratorReasonCopy, modelLicenses, version label, About facts, models and licenses
 * WHAT:  What Settings → About says: the version line, the speech engine's state in a few words (`engineSummary`),
 *        why it runs where it does (`acceleratorReasonCopy`), and every model and runtime Echo can use with its license
 *        and credit, each once (`modelLicenses`).
 * WHY:   The words live next to About and nowhere else; every mapping is keyed by a generated union, so a new
 *        readiness or reason fails tsc until it has copy. Licenses come from the registry manifests Rust returns in the
 *        Models view (05 A15: Parakeet is CC-BY-4.0 and needs its attribution shown), including the runtimes a model
 *        requires; a manifest shared by two engines is listed once. Pure, so it is tested alone.
 * WHERE: routes/settings/_components/AboutSection.tsx.
 */
import type { AcceleratorReason, AppInfo, EngineSpec, ModelManifest, ModelsView, SpeechEngineStatus } from "@/bindings";
import { describeAppError } from "@/lib/app-error";

/** "0.1.0", with a note on a development build. */
export function versionLabel(app: AppInfo): string {
  return app.development ? `${app.version} (development build)` : app.version;
}

/** The engine's name and state: its label when ready, otherwise what it is doing or why it cannot run. */
export function engineSummary(status: SpeechEngineStatus, engines: readonly EngineSpec[]): string {
  const readiness = status.readiness;
  if (readiness.kind === "unloaded") {
    return "Not loaded yet";
  }
  const label = engines.find((engine) => engine.id === readiness.engine_id)?.label ?? readiness.engine_id;
  switch (readiness.kind) {
    case "ready":
      return label;
    case "loading":
      return `${label} (loading)`;
    case "failed":
      return `${label}: ${describeAppError(readiness.error).title}`;
    default:
      return readiness satisfies never;
  }
}

const REASON_COPY: Readonly<Record<AcceleratorReason["kind"], string>> = {
  only_option: "The only processor this engine supports.",
  preference: "Chosen in Settings.",
  no_gpu: "No compatible graphics card was found.",
  gpu_failed: "The graphics card couldn't run it, so the processor took over.",
  measuring: "Checking in the background whether the graphics card is faster.",
  measured: "Measured as the faster choice on this PC.",
  remembered: "Measured earlier as the faster choice on this PC.",
  gpu_lost: "The graphics card stopped responding, so the processor took over.",
};

/** Why the engine runs where it does, in one sentence. */
export function acceleratorReasonCopy(reason: AcceleratorReason): string {
  return REASON_COPY[reason.kind];
}

/** Every model and runtime in the Models view, each once, in registry order. */
export function modelLicenses(models: ModelsView): ModelManifest[] {
  const seen = new Set<string>();
  return models.entries
    .flatMap((entry) => [entry.model, ...entry.requires])
    .filter((manifest) => {
      if (seen.has(manifest.id)) {
        return false;
      }
      seen.add(manifest.id);
      return true;
    });
}
