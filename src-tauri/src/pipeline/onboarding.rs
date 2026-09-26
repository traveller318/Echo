/*!
 * SOURCE OF TRUTH KEYWORDS: onboarding pipeline, onboarding view, OnboardingView build, onboarding required, speech model ready, microphone consent, complete onboarding, general.onboarded
 * WHAT:  `view(settings, models, consent)`: the OnboardingView Echo shows now (whether onboarding is required, the
 *        steps to walk, the selected speech engine, whether its model is ready, the Windows microphone consent, the
 *        dictation hotkey as bound now and whether it is held);
 *        `needs(settings, models)`: why onboarding is due; `complete(..)`: remembers that onboarding was finished
 *        (`general.onboarded`, hidden) and announces it.
 * WHY:   02 §8.2 and 01 §7: onboarding is shown on first run and whenever the active speech model is missing, so the
 *        rule reads the one ModelsView the Models page renders (same combined status, damage included) and the
 *        registry decides which steps apply (registry/onboarding.rs). Consent comes from the registry permission
 *        check, the same answer the factory preflight gives `audio_test_level`; a consent store that cannot be read
 *        is logged and reported as unknown, never guessed. Completion is a hidden setting written through the
 *        settings store, so it survives restarts in the one settings table and every window hears SettingsChanged.
 * WHERE: ipc/commands/onboarding.rs (`onboarding_get`, `onboarding_complete`).
 */

use crate::{
    pipeline::settings_store,
    ports::{EventSink, PrivacyConsent},
    registry::{self, permissions::PermissionCtx, settings::keys},
    services::Db,
    types::{
        AppEvent, HotkeyAction, ModelsView, OnboardingNeeds, OnboardingView, Permission,
        PermissionState, PortResult, RecordMode, SettingValue, SettingsSnapshot, SharedSettings,
    },
};

/// The onboarding view for the settings in effect, the models as the Models page sees them and the consent now.
pub fn view(
    settings: &SettingsSnapshot,
    models: &ModelsView,
    consent: &dyn PrivacyConsent,
) -> OnboardingView {
    let needs = needs(settings, models);
    OnboardingView {
        required: needs.required(),
        first_run: needs.first_run,
        speech_engine: registry::settings::asr_engine(settings),
        speech_model_ready: !needs.speech_model_missing,
        microphone: microphone_consent(settings, consent),
        record_hotkey: registry::hotkeys::HOTKEYS
            .iter()
            .find(|spec| spec.action == HotkeyAction::Record)
            .map(|spec| registry::hotkeys::effective_shortcut(spec, settings)),
        hold_to_talk: registry::settings::record_mode(settings) == RecordMode::Hold,
        steps: registry::onboarding::steps_for(needs),
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: onboarding needs, onboarding due, first run check, speech model missing check
 * WHAT:  Why onboarding is due now: never completed, and/or the selected speech engine's model is not ready.
 * WHY:   One rule for the view and for anything that must act on it without the UI (showing the main window at
 *        launch even when Echo starts in the tray, so a first run never hides its setup). An engine without a
 *        model card has nothing to download, so it counts as ready.
 * WHERE: `view`; the launch-visibility decision that joins with startup behaviour (step 25).
 */
pub fn needs(settings: &SettingsSnapshot, models: &ModelsView) -> OnboardingNeeds {
    let speech_model_ready = registry::settings::asr_engine(settings)
        .as_ref()
        .is_none_or(|engine| {
            models
                .entries
                .iter()
                .find(|entry| entry.engine.id == *engine)
                .is_none_or(|entry| entry.status.is_installed())
        });
    OnboardingNeeds {
        first_run: !registry::settings::onboarded(settings),
        speech_model_missing: !speech_model_ready,
    }
}

/// Windows microphone consent through the registry permission check; None (logged) when it cannot be read.
fn microphone_consent(
    settings: &SettingsSnapshot,
    consent: &dyn PrivacyConsent,
) -> Option<PermissionState> {
    let ctx = PermissionCtx { settings, consent };
    match registry::permissions::check(Permission::Microphone, &ctx) {
        Ok(state) => Some(state),
        Err(error) => {
            tracing::warn!(
                detail = error.detail(),
                "the microphone consent could not be read for onboarding"
            );
            None
        }
    }
}

/// What finishing onboarding writes through.
pub struct OnboardingStore<'a> {
    pub settings: &'a SharedSettings,
    pub db: &'a Db,
    pub events: &'a dyn EventSink<AppEvent>,
}

/// Remembers that onboarding was finished; SettingsChanged tells every window.
pub fn complete(store: &OnboardingStore<'_>) -> PortResult<()> {
    settings_store::store_internal(
        store.settings,
        store.db,
        store.events,
        &keys::ONBOARDED,
        SettingValue::Bool(true),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::{FakePrivacyConsent, RecordingSink},
        registry::engines::PARAKEET_TDT_V3,
        types::{
            AppError, ByteCount, EngineSelection, ModelEntry, ModelStatus, OnboardingStepId,
            PortError, Shortcut, StaticStr,
        },
    };

    /// The Models page view with the Parakeet card in `status`.
    fn models(status: ModelStatus) -> ModelsView {
        let (entry, manifest) = registry::engines::with_models()
            .find(|(entry, _)| entry.id == PARAKEET_TDT_V3)
            .unwrap();
        ModelsView {
            entries: vec![ModelEntry {
                engine: entry.spec(),
                model: manifest.clone(),
                requires: Vec::new(),
                download_bytes: ByteCount::new(0),
                status,
                selection: EngineSelection::Selectable { active: true },
                runtime: None,
                transfer: None,
            }],
            network: PermissionState::Granted,
        }
    }

    fn step_ids(view: &OnboardingView) -> Vec<OnboardingStepId> {
        view.steps.iter().map(|step| step.id).collect()
    }

    #[test]
    fn a_fresh_install_without_the_model_walks_every_step() {
        let view = view(
            &registry::settings::defaults(),
            &models(ModelStatus::NotInstalled),
            &FakePrivacyConsent::granted(),
        );
        assert!(view.required && view.first_run);
        assert!(!view.speech_model_ready);
        assert_eq!(view.speech_engine, Some(PARAKEET_TDT_V3));
        assert_eq!(view.microphone, Some(PermissionState::Granted));
        assert_eq!(
            view.record_hotkey.as_ref().map(Shortcut::as_str),
            Some(registry::hotkeys::RECORD_DEFAULT)
        );
        assert!(view.hold_to_talk, "hold to talk is the default mode");
        assert_eq!(
            step_ids(&view),
            [
                OnboardingStepId::Microphone,
                OnboardingStepId::Model,
                OnboardingStepId::Hotkey,
                OnboardingStepId::Practice,
            ]
        );
    }

    #[test]
    fn an_installed_model_skips_its_step_and_a_finished_onboarding_is_not_required() {
        let installed = models(ModelStatus::Installed);
        let first = view(
            &registry::settings::defaults(),
            &installed,
            &FakePrivacyConsent::granted(),
        );
        assert!(first.required && first.speech_model_ready);
        assert!(!step_ids(&first).contains(&OnboardingStepId::Model));

        let onboarded = registry::settings::resolve([(keys::ONBOARDED, SettingValue::Bool(true))]);
        let settled = view(&onboarded, &installed, &FakePrivacyConsent::granted());
        assert!(!settled.required);
        assert!(settled.steps.is_empty());
    }

    #[test]
    fn a_missing_partial_or_damaged_model_brings_onboarding_back() {
        let onboarded = registry::settings::resolve([(keys::ONBOARDED, SettingValue::Bool(true))]);
        for status in [
            ModelStatus::NotInstalled,
            ModelStatus::Partial {
                bytes: ByteCount::new(10),
            },
            ModelStatus::Corrupt,
        ] {
            let view = view(&onboarded, &models(status), &FakePrivacyConsent::granted());
            assert!(view.required && !view.first_run, "{status:?}");
            assert_eq!(
                step_ids(&view),
                [OnboardingStepId::Model, OnboardingStepId::Practice],
                "{status:?}"
            );
        }
    }

    #[test]
    fn the_hotkey_instructions_follow_the_settings_in_effect() {
        let chosen = registry::settings::resolve([
            (
                keys::RECORD_HOTKEY,
                SettingValue::Hotkey(StaticStr::new("Ctrl+Shift+Space")),
            ),
            (
                keys::HOTKEY_MODE,
                SettingValue::Enum(StaticStr::new(registry::settings::values::TOGGLE)),
            ),
        ]);
        let view = view(
            &chosen,
            &models(ModelStatus::Installed),
            &FakePrivacyConsent::granted(),
        );
        assert_eq!(
            view.record_hotkey.as_ref().map(Shortcut::as_str),
            Some("Ctrl+Shift+Space")
        );
        assert!(!view.hold_to_talk);
    }

    #[test]
    fn microphone_consent_is_reported_or_left_unknown() {
        let installed = models(ModelStatus::Installed);
        let settings = registry::settings::defaults();
        let blocked = view(
            &settings,
            &installed,
            &FakePrivacyConsent::new(PermissionState::Denied),
        );
        assert_eq!(blocked.microphone, Some(PermissionState::Denied));

        let unreadable = FakePrivacyConsent::granted();
        unreadable.fail_next(PortError::new(AppError::Internal).with_detail("registry locked"));
        assert_eq!(view(&settings, &installed, &unreadable).microphone, None);
    }

    #[test]
    fn completing_onboarding_is_remembered_and_announced() {
        let settings = SharedSettings::new(registry::settings::defaults());
        let db = Db::open_in_memory().unwrap();
        let events = RecordingSink::default();
        complete(&OnboardingStore {
            settings: &settings,
            db: &db,
            events: &events,
        })
        .unwrap();
        assert!(registry::settings::onboarded(&settings.current()));
        assert_eq!(events.events().len(), 1);
        let after = view(
            &settings.current(),
            &models(ModelStatus::Installed),
            &FakePrivacyConsent::granted(),
        );
        assert!(!after.required);
    }
}
