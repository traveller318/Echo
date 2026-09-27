/**
 * SOURCE OF TRUTH KEYWORDS: pillKind, PillKind, PILL_WIDTH_TOKENS, pillWidthToken, pill state mapping, pill morph width, SessionView to pill, idle pill
 * WHAT:  `pillKind(view, look)`: which of the pill's layouts (04 §4) a SessionView shows, or null when the pill is gone;
 *        `pillWidthToken(kind, style)`: the --pill-width-* token of a layout in a pill style (PILL_WIDTH_TOKENS for the
 *        layouts every style shares).
 * WHY:   The pill renders only what Rust sent (SessionStateChanged, PillLookChanged), so this is a pure projection
 *        with no state of its own and is table-tested. Arming already looks like Recording (the pill appears on the
 *        press, 02 §6.2). The pill leaves as soon as the take stops: Finalizing and Delivering show nothing, because a
 *        "Transcribing" layout on every take was noise and the text landing is the confirmation (owner request,
 *        2026-09-27). A discarded take hides the pill (02 §5), and so does a pasted or rehearsed one; only "Copied"
 *        and "No speech detected" bring it back with words. A failed take whose model is missing gets its own
 *        "Set up" layout; every other failure is the error layout. Whenever a take shows nothing, a pill the user
 *        keeps on screen (`pill.visibility` = always) rests in its idle layout instead of leaving. Only the idle and
 *        recording layouts differ per style (PILL_STYLES); words need the same width in every style.
 * WHERE: src/pill/Pill.tsx.
 */
import type { DeliveryOutcome, PillLook, PillStyle, SessionView } from "@/bindings";
import { PILL_STYLES } from "@/components/global";

export type PillKind = "idle" | "recording" | "cancel" | "copied" | "no_speech" | "error" | "model_missing";

/** The layouts whose width follows the pill style. */
type StyledKind = Extract<PillKind, "idle" | "recording">;

/** The width token of each layout every style shares (docs/04 §4, §3.10). */
export const PILL_WIDTH_TOKENS = {
  cancel: "--pill-width-cancel",
  copied: "--pill-width-copied",
  no_speech: "--pill-width-notice",
  error: "--pill-width-error",
  model_missing: "--pill-width-setup",
} as const satisfies Readonly<Record<Exclude<PillKind, StyledKind>, string>>;

function isStyledKind(kind: PillKind): kind is StyledKind {
  return kind === "idle" || kind === "recording";
}

export function pillWidthToken(kind: PillKind, style: PillStyle): string {
  if (!isStyledKind(kind)) {
    return PILL_WIDTH_TOKENS[kind];
  }
  const spec = PILL_STYLES[style];
  return kind === "idle" ? spec.restWidth : spec.recordingWidth;
}

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

/** The layout the take itself calls for, or null when it shows nothing. */
function takeKind(view: SessionView): PillKind | null {
  switch (view.status) {
    // Finalizing / Delivering: the take stopped, so the pill leaves while its text is transcribed and delivered.
    case "idle":
    case "discarded":
    case "finalizing":
    case "delivering":
      return null;
    case "arming":
    case "recording":
      return "recording";
    case "cancel_pending":
      return "cancel";
    case "done":
      return view.outcome === null ? null : DONE_KIND[view.outcome];
    case "failed":
      return view.error?.code === "ModelMissing" ? "model_missing" : "error";
  }
}

export function pillKind(view: SessionView | null, look: PillLook | null): PillKind | null {
  if (view === null) {
    return null;
  }
  return takeKind(view) ?? (look?.visibility === "always" ? "idle" : null);
}
