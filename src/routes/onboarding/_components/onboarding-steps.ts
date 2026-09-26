/**
 * SOURCE OF TRUTH KEYWORDS: ONBOARDING_STEP_PAGES, OnboardingStepPage, onboarding step table, step title, step canContinue, step component per id
 * WHAT:  The page of every onboarding step id: its component, title, one-sentence explanation and whether the user
 *        may move on now (`canContinue`, read from Rust's onboarding view).
 * WHY:   Steps are registry entries (registry/onboarding.rs); this table is keyed by the generated OnboardingStepId
 *        union, so a new step id fails tsc until it has a page here (the NAV_PAGES pattern). Only the model step
 *        holds the user back: without the model no take can work, so Continue waits until Rust reports it ready;
 *        every other step can be passed and fixed later from Settings. Kept in a .ts file because a file that
 *        defines components may export nothing else (fast refresh).
 * WHERE: OnboardingFlow (this folder).
 */
import type { ComponentType } from "react";
import type { OnboardingStepId, OnboardingView } from "@/bindings";
import { HotkeyStep } from "./HotkeyStep";
import { MicrophoneStep } from "./MicrophoneStep";
import { ModelStep } from "./ModelStep";
import { PracticeStep } from "./PracticeStep";
import type { OnboardingStepProps } from "./step-props";

export interface OnboardingStepPage {
  readonly Component: ComponentType<OnboardingStepProps>;
  readonly title: string;
  readonly body: string;
  /** The user may move on from this step now. */
  readonly canContinue: (view: OnboardingView) => boolean;
}

const ALWAYS = () => true;

export const ONBOARDING_STEP_PAGES: Readonly<Record<OnboardingStepId, OnboardingStepPage>> = {
  microphone: {
    Component: MicrophoneStep,
    title: "Check your microphone",
    body: "Echo listens only while you use the hotkey, and everything stays on this PC.",
    canContinue: ALWAYS,
  },
  model: {
    Component: ModelStep,
    title: "Get the speech model",
    body: "Speech recognition runs on this PC. Download the model once, or import it from a folder.",
    canContinue: (view) => view.speech_model_ready,
  },
  hotkey: {
    Component: HotkeyStep,
    title: "Try your hotkey",
    body: "The hotkey starts dictation from any app. Testing it here records nothing.",
    canContinue: ALWAYS,
  },
  practice: {
    Component: PracticeStep,
    title: "Try it",
    body: "Dictate one sentence. The text shows up here instead of being pasted.",
    canContinue: ALWAYS,
  },
};
