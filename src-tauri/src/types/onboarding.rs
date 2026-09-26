/*!
 * SOURCE OF TRUTH KEYWORDS: OnboardingStepId, OnboardingCondition, OnboardingStepSpec, OnboardingNeeds, OnboardingView, onboarding_get, first run, speech model missing, onboarding steps
 * WHAT:  The shapes of first-run onboarding (01 §7, 04 §5): the closed set of steps (OnboardingStepId), when a
 *        step belongs to onboarding (OnboardingCondition), a registry step entry (OnboardingStepSpec), why
 *        onboarding is needed now (OnboardingNeeds) and everything the onboarding screen renders from
 *        (OnboardingView: whether it is required, the steps to walk, the speech engine and its model's state, the
 *        microphone consent, the dictation hotkey and its mode).
 * WHY:   Steps are registry entries (registry/onboarding.rs), so adding one is an entry plus its page, never a
 *        new flow: the id is an enum because the UI owns one step component per id, and the generated TS union
 *        makes a new id fail tsc until its component exists (the NavId pattern). A step names the settings it offers
 *        by registry key, so the UI renders their generated controls and never names a setting itself. Rust decides whether onboarding
 *        is required and which steps apply (a model step only while the model is missing), so the UI renders the
 *        answer and never re-derives a rule. The microphone consent is None when Windows could not be asked: the
 *        UI says so instead of guessing (05 W13).
 * WHERE: ONBOARDING_STEPS in registry/onboarding.rs; built by pipeline/onboarding.rs; returned by `onboarding_get`
 *        and `onboarding_complete` (ipc/commands/onboarding.rs); rendered by src/routes/onboarding.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{EngineId, PermissionState, SettingKey, Shortcut, StaticList, StaticStr};

/// One onboarding step; each has a component in `src/routes/onboarding/_components/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStepId {
    /// Windows microphone consent, the privacy deep link and a live level meter.
    Microphone,
    /// Download or import the speech model.
    Model,
    /// Dictate with the hotkey into a practice pad in Echo (text shown, never pasted); rebind a clashing hotkey.
    Practice,
}

impl OnboardingStepId {
    /// The wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::Model => "model",
            Self::Practice => "practice",
        }
    }
}

/// When a step is part of onboarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingCondition {
    /// Until onboarding has been completed once.
    FirstRun,
    /// Whenever the selected speech engine's model is not ready (first run or later).
    SpeechModelMissing,
    /// Every time onboarding is shown.
    Always,
}

/// A registry onboarding step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OnboardingStepSpec {
    pub id: OnboardingStepId,
    /// Short name for the dot indicator's accessible label, e.g. "Microphone".
    pub label: StaticStr,
    /// When the step is walked.
    pub shown: OnboardingCondition,
    /// The settings the step lets the user change on the spot, in order (rendered from their registry specs).
    pub settings: StaticList<SettingKey>,
}

/// Why onboarding is needed now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct OnboardingNeeds {
    /// Onboarding has never been completed (`general.onboarded` is off).
    pub first_run: bool,
    /// The selected speech engine's model is not ready (not installed, partly downloaded or damaged).
    pub speech_model_missing: bool,
}

impl OnboardingNeeds {
    /// Onboarding should be shown.
    pub const fn required(self) -> bool {
        self.first_run || self.speech_model_missing
    }

    /// `condition` holds for these needs.
    pub const fn meets(self, condition: OnboardingCondition) -> bool {
        match condition {
            OnboardingCondition::FirstRun => self.first_run,
            OnboardingCondition::SpeechModelMissing => self.speech_model_missing,
            OnboardingCondition::Always => true,
        }
    }
}

/// Everything the onboarding screen renders from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OnboardingView {
    /// Onboarding should be shown: it was never completed, or the speech model is not ready.
    pub required: bool,
    /// Onboarding has never been completed.
    pub first_run: bool,
    /// The speech engine the settings select (its Models card is the model step); None when none is registered.
    pub speech_engine: Option<EngineId>,
    /// The selected speech engine's model is installed and undamaged (true when it needs no model).
    pub speech_model_ready: bool,
    /// Windows microphone consent for desktop apps; None when it could not be read.
    pub microphone: Option<PermissionState>,
    /// The dictation hotkey as it is bound now (for the practice step's instructions).
    pub record_hotkey: Option<Shortcut>,
    /// The dictation hotkey is held while speaking (hold mode); false: press to start, press again to stop.
    pub hold_to_talk: bool,
    /// The steps to walk now, in order; empty when onboarding is not required.
    pub steps: Vec<OnboardingStepSpec>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn needs_decide_whether_onboarding_is_required_and_which_conditions_hold() {
        let settled = OnboardingNeeds::default();
        assert!(!settled.required());
        assert!(settled.meets(OnboardingCondition::Always));
        assert!(!settled.meets(OnboardingCondition::FirstRun));

        let first_run = OnboardingNeeds {
            first_run: true,
            speech_model_missing: false,
        };
        assert!(first_run.required());
        assert!(first_run.meets(OnboardingCondition::FirstRun));
        assert!(!first_run.meets(OnboardingCondition::SpeechModelMissing));

        let missing = OnboardingNeeds {
            first_run: false,
            speech_model_missing: true,
        };
        assert!(missing.required());
        assert!(missing.meets(OnboardingCondition::SpeechModelMissing));
        assert!(!missing.meets(OnboardingCondition::FirstRun));
    }

    #[test]
    fn step_ids_use_snake_case_on_the_wire() {
        for id in [
            OnboardingStepId::Microphone,
            OnboardingStepId::Model,
            OnboardingStepId::Practice,
        ] {
            assert_eq!(serde_json::to_value(id).unwrap(), json!(id.as_str()));
        }
        assert_eq!(
            serde_json::to_value(OnboardingCondition::SpeechModelMissing).unwrap(),
            json!("speech_model_missing")
        );
    }
}
