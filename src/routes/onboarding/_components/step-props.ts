/**
 * SOURCE OF TRUTH KEYWORDS: OnboardingStepProps, onboarding step contract, step component props
 * WHAT:  What every onboarding step component receives: the onboarding view Rust sent last and its own registry
 *        step entry.
 * WHY:   Steps render from Rust's answer only (whether the model is ready, the consent, the hotkey) and from their
 *        registry entry (the settings they offer), so a step keeps no copy of domain state.
 * WHERE: onboarding-steps.ts (the step table) and every step component in this folder.
 */
import type { OnboardingStepSpec, OnboardingView } from "@/bindings";

export interface OnboardingStepProps {
  readonly view: OnboardingView;
  readonly step: OnboardingStepSpec;
}
