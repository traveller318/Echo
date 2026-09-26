/**
 * SOURCE OF TRUTH KEYWORDS: MIC_VERDICT_LOOK, micVerdictLook, microphone verdict copy, mic check result copy, too quiet, no signal, too loud
 * WHAT:  What each microphone check verdict says: a short title, one sentence on what to do, and whether it passed.
 * WHY:   Rust judges the levels (pipeline/capture/level.rs); the UI only words the verdict, once, in calm copy (04 §1).
 *        The table is keyed by the generated MicVerdict union, so a new verdict fails tsc until it has copy.
 * WHERE: MicCheckPanel (this folder).
 */
import type { MicVerdict } from "@/bindings";

export interface MicVerdictLook {
  readonly title: string;
  readonly body: string;
  /** The microphone is fine for dictation. */
  readonly passed: boolean;
}

export const MIC_VERDICT_LOOK: Readonly<Record<MicVerdict, MicVerdictLook>> = {
  good: { title: "Sounds good", body: "Echo hears you clearly.", passed: true },
  too_quiet: {
    title: "A little quiet",
    body: "Move closer to the microphone, or raise its level in Windows sound settings.",
    passed: false,
  },
  too_loud: {
    title: "Too loud",
    body: "Your voice clips. Move back a little, or lower the microphone level.",
    passed: false,
  },
  no_signal: {
    title: "Nothing heard",
    body: "The microphone sends silence. Check that it isn't muted and that microphone access is on.",
    passed: false,
  },
};
