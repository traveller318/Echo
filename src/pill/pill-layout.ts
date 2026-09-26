/**
 * SOURCE OF TRUTH KEYWORDS: pillKind, PillKind, PILL_WIDTH_TOKENS, pill state mapping, pill morph width, SessionView to pill
 * WHAT:  `pillKind(view, processingShown)`: which of the pill's layouts (04 §4) a SessionView shows, or null when the
 *        pill is gone; PILL_WIDTH_TOKENS: the --pill-width-* token of each layout; `isFinishing(view)`: the take
 *        stopped and its text is on its way (the processing wait).
 * WHY:   The pill renders only what Rust sent (SessionStateChanged), so this is a pure projection with no state of its
 *        own and is table-tested. Arming already looks like Recording (the pill appears on the press, 02 §6.2). The
 *        stop → paste path usually finishes inside --delay-loading, so Finalizing and Delivering keep the Recording
 *        layout until the delay passed (04 §1 "Nothing waits"). A discarded take hides the pill (02 §5), and so does a pasted or rehearsed one: the text is the
 *        confirmation, only "Copied" and "No speech detected" need words. A failed
 *        take whose model is missing gets its own "Set up" layout; every other failure is the error layout.
 * WHERE: src/pill/Pill.tsx.
 */
import type { DeliveryOutcome, SessionView } from "@/bindings";

export type PillKind =
  | "recording"
  | "cancel"
  | "processing"
  | "copied"
  | "no_speech"
  | "error"
  | "model_missing";

/** The width token of each layout (docs/04 §4, §3.10). */
export const PILL_WIDTH_TOKENS = {
  recording: "--pill-width-recording",
  cancel: "--pill-width-cancel",
  processing: "--pill-width-processing",
  copied: "--pill-width-copied",
  no_speech: "--pill-width-notice",
  error: "--pill-width-error",
  model_missing: "--pill-width-setup",
} as const satisfies Readonly<Record<PillKind, string>>;

/**
 * The layout of a delivered take, per outcome (null: the pill just leaves); keyed by the generated union, so a new
 * outcome fails tsc here.
 */
const DONE_KIND: Readonly<Record<DeliveryOutcome, PillKind | null>> = {
  // The text landing where the user typed is the confirmation; a ✓ on every take was noise.
  pasted: null,
  copied: "copied",
  no_speech: "no_speech",
  // A rehearsed take (onboarding's practice): the text is in Echo's practice pad, which confirms it.
  shown: null,
};

/** The take stopped and its text is being transcribed or delivered. */
export function isFinishing(view: SessionView | null): boolean {
  return view?.status === "finalizing" || view?.status === "delivering";
}

export function pillKind(view: SessionView | null, processingShown: boolean): PillKind | null {
  if (view === null) {
    return null;
  }
  switch (view.status) {
    case "idle":
    case "discarded":
      return null;
    case "arming":
    case "recording":
      return "recording";
    case "cancel_pending":
      return "cancel";
    case "finalizing":
    case "delivering":
      return processingShown ? "processing" : "recording";
    case "done":
      return view.outcome === null ? null : DONE_KIND[view.outcome];
    case "failed":
      return view.error?.code === "ModelMissing" ? "model_missing" : "error";
  }
}
