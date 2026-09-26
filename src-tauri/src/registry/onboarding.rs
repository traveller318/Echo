/*!
 * SOURCE OF TRUTH KEYWORDS: onboarding registry, ONBOARDING_STEPS, onboarding steps, steps_for, microphone step, model step, practice step, hotkey settings, first run
 * WHAT:  The onboarding steps in the order they are walked (Microphone → Model → Try it, 04 §5), each with the
 *        condition that puts it in onboarding and the settings it offers on the spot (the input device; the
 *        dictation hotkey and its mode), and `steps_for(needs)`: the steps to walk now.
 * WHY:   A step is a registry entry (02 §3.3), so adding or reordering one is an edit here plus its UI component,
 *        never a change to the flow. The conditions carry 01 §7 and the step-24 rules: on first run every step is
 *        walked, but the model step only while the model is missing ("skipped if already installed"); when an
 *        onboarded user's model goes missing, onboarding returns with just the model step and a practice take to
 *        prove the model works, so a returning user is never asked to test the microphone again. The hotkey is
 *        tested by the practice take itself (a take that starts proves the hotkey), so "Try it" offers the hotkey
 *        and its mode for rebinding a clash on the spot rather than walking a separate press-only test.
 * WHERE: Read by pipeline/onboarding.rs (the OnboardingView of `onboarding_get` / `onboarding_complete`).
 */

use super::settings::keys;
use crate::types::{
    OnboardingCondition, OnboardingNeeds, OnboardingStepId, OnboardingStepSpec, SettingKey,
    StaticList, StaticStr,
};

/// The microphone step offers the input device (a const of its own: SettingKey has drop glue).
const MICROPHONE_SETTINGS: &[SettingKey] = &[keys::INPUT_DEVICE];

/// The practice step offers the dictation hotkey and whether it is held or toggled.
const PRACTICE_SETTINGS: &[SettingKey] = &[keys::RECORD_HOTKEY, keys::HOTKEY_MODE];

const NO_SETTINGS: &[SettingKey] = &[];

/// Every onboarding step, in the order it is walked.
pub const ONBOARDING_STEPS: &[OnboardingStepSpec] = &[
    OnboardingStepSpec {
        id: OnboardingStepId::Microphone,
        label: StaticStr::new("Microphone"),
        shown: OnboardingCondition::FirstRun,
        settings: StaticList::new(MICROPHONE_SETTINGS),
    },
    OnboardingStepSpec {
        id: OnboardingStepId::Model,
        label: StaticStr::new("Speech model"),
        shown: OnboardingCondition::SpeechModelMissing,
        settings: StaticList::new(NO_SETTINGS),
    },
    OnboardingStepSpec {
        id: OnboardingStepId::Practice,
        label: StaticStr::new("Try it"),
        shown: OnboardingCondition::Always,
        settings: StaticList::new(PRACTICE_SETTINGS),
    },
];

/// The steps to walk for `needs`, in order; none when onboarding is not required.
pub fn steps_for(needs: OnboardingNeeds) -> Vec<OnboardingStepSpec> {
    if !needs.required() {
        return Vec::new();
    }
    ONBOARDING_STEPS
        .iter()
        .filter(|step| needs.meets(step.shown))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn ids(needs: OnboardingNeeds) -> Vec<OnboardingStepId> {
        steps_for(needs).into_iter().map(|step| step.id).collect()
    }

    #[test]
    fn step_ids_are_unique_labelled_and_offer_only_visible_registered_settings() {
        let mut seen = HashSet::new();
        for step in ONBOARDING_STEPS {
            assert!(seen.insert(step.id), "duplicate step {:?}", step.id);
            assert!(!step.label.trim().is_empty(), "{:?} has no label", step.id);
            for key in step.settings.iter() {
                let spec = crate::registry::settings::find(key);
                assert!(
                    spec.is_some_and(|spec| spec.visible),
                    "{:?} offers {key}, which is not a visible setting",
                    step.id
                );
            }
        }
    }

    #[test]
    fn a_first_run_walks_every_step_and_skips_an_installed_model() {
        use OnboardingStepId::{Microphone, Model, Practice};
        let fresh = OnboardingNeeds {
            first_run: true,
            speech_model_missing: true,
        };
        assert_eq!(ids(fresh), [Microphone, Model, Practice]);
        let installed = OnboardingNeeds {
            first_run: true,
            speech_model_missing: false,
        };
        assert_eq!(ids(installed), [Microphone, Practice]);
    }

    #[test]
    fn a_missing_model_later_brings_back_only_the_model_and_a_practice_take() {
        let missing = OnboardingNeeds {
            first_run: false,
            speech_model_missing: true,
        };
        assert_eq!(
            ids(missing),
            [OnboardingStepId::Model, OnboardingStepId::Practice]
        );
        assert!(ids(OnboardingNeeds::default()).is_empty());
    }
}
