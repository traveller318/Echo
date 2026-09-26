/*!
 * SOURCE OF TRUTH KEYWORDS: onboarding commands, onboarding_get, onboarding_complete, onboarding_open, OnboardingView, OnboardingRequested, first-run onboarding, pill Set up
 * WHAT:  The onboarding command group: `onboarding_get` returns the OnboardingView (required or not, the steps to
 *        walk, the speech engine and whether its model is ready, the microphone consent); `onboarding_complete`
 *        remembers that onboarding was finished, turns any session rehearsal off and returns the view now in
 *        effect; `onboarding_open` brings the main window forward and asks it (OnboardingRequested) to show
 *        onboarding.
 * WHY:   Rust decides whether onboarding is due and which steps apply (pipeline/onboarding.rs, registry/onboarding.rs);
 *        the UI renders the answer and stays fresh from ModelsChanged and SettingsChanged. The view reads disk sizes
 *        through the model manager's list, so it runs on the blocking pool like `models_list`. Finishing also ends
 *        the rehearsal onboarding may have left on, so dictation behaves normally at once. `onboarding_open` serves
 *        the pill's "Model not installed · Set up" (no router, no rights on the main window): the window is shown
 *        first, so the event reaches a live page.
 * WHERE: Registered through `ipc::commands::catalog`; called by the main window's onboarding gate and route
 *        (src/app/onboarding-gate.tsx, src/routes/onboarding) and by the pill (src/pill/pill-commands.ts).
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::{
        blocking::run_blocking,
        onboarding::{self, OnboardingStore},
    },
    types::{OnboardingRequested, OnboardingView, PortError, SessionRehearsal},
};

echo_command! {
    /// Whether onboarding is due now, the steps to walk, the speech engine and its model's state, and the Windows
    /// microphone consent.
    name: onboarding_get,
    output: OnboardingView,
    permission: None,
    reentrancy: Shared,
    handler: get,
}

echo_command! {
    /// Remembers that onboarding was finished, turns the session rehearsal off and returns the view now in effect.
    name: onboarding_complete,
    output: OnboardingView,
    permission: None,
    reentrancy: Shared,
    handler: complete,
}

echo_command! {
    /// Brings the main window forward on onboarding (the pill's "Set up").
    name: onboarding_open,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: open,
}

/// The onboarding view for the settings, models and consent in effect now.
pub async fn get(ctx: &CommandCtx, (): ()) -> Result<OnboardingView, PortError> {
    let models = ctx.models().clone();
    let listed = run_blocking("listing models for onboarding", move || models.list()).await?;
    Ok(onboarding::view(&ctx.settings(), &listed, ctx.consent()))
}

/// Stores the completion, ends any rehearsal, then answers the view that follows.
pub async fn complete(ctx: &CommandCtx, (): ()) -> Result<OnboardingView, PortError> {
    onboarding::complete(&OnboardingStore {
        settings: ctx.shared_settings(),
        db: ctx.db(),
        events: ctx.events(),
    })?;
    ctx.session().rehearse(SessionRehearsal::Off)?;
    get(ctx, ()).await
}

/// Shows the main window, then asks it to open onboarding.
pub async fn open(ctx: &CommandCtx, (): ()) -> Result<(), PortError> {
    ctx.main_window().show()?;
    ctx.emit(OnboardingRequested {});
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::FakePrivacyConsent,
        registry::{self, settings::keys},
        types::{
            AppError, AppEvent, CommandSpec, OnboardingStepId, PermissionState, Reentrancy,
            SettingValue, SettingsChanged,
        },
    };

    const fn spec(name: &'static str) -> CommandSpec {
        CommandSpec {
            name,
            permission: None,
            reentrancy: Reentrancy::Shared,
        }
    }

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn a_fresh_install_needs_every_step_and_reports_the_consent() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::new(PermissionState::Denied),
        );
        let view = block_on(factory::run(&harness.ctx, &spec("onboarding_get"), (), get)).unwrap();
        assert!(view.required && view.first_run && !view.speech_model_ready);
        assert_eq!(view.microphone, Some(PermissionState::Denied));
        assert_eq!(
            view.steps.iter().map(|step| step.id).collect::<Vec<_>>(),
            [
                OnboardingStepId::Microphone,
                OnboardingStepId::Model,
                OnboardingStepId::Practice,
            ]
        );
    }

    #[test]
    fn completing_is_remembered_announced_and_still_asks_for_a_missing_model() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        // The session actor is dropped: it is not running, so ending its rehearsal must still be possible.
        let testing::Harness {
            ctx,
            session_actor,
            events,
            ..
        } = harness;
        drop(session_actor);
        let result = block_on(factory::run(
            &ctx,
            &spec("onboarding_complete"),
            (),
            complete,
        ));
        assert_eq!(
            result.err(),
            Some(AppError::Internal),
            "the rehearsal cannot be ended without an actor"
        );
        assert!(
            registry::settings::onboarded(&ctx.settings()),
            "the completion is stored before the actor is asked"
        );
        assert!(
            events
                .events()
                .contains(&AppEvent::SettingsChanged(SettingsChanged {
                    key: keys::ONBOARDED,
                    value: SettingValue::Bool(true),
                }))
        );
    }

    #[test]
    fn completing_with_a_session_returns_the_view_that_follows() {
        // The harness keeps its (idle) session actor, so the rehearsal message is queued for it.
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let view = block_on(factory::run(
            &harness.ctx,
            &spec("onboarding_complete"),
            (),
            complete,
        ))
        .unwrap();
        assert!(!view.first_run);
        assert!(
            view.required,
            "the harness has no model installed, so onboarding still asks for it"
        );
        assert_eq!(
            view.steps.iter().map(|step| step.id).collect::<Vec<_>>(),
            [OnboardingStepId::Model, OnboardingStepId::Practice]
        );
    }

    #[test]
    fn open_shows_the_main_window_then_asks_it_for_onboarding() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let result = block_on(factory::run(
            &harness.ctx,
            &spec("onboarding_open"),
            (),
            open,
        ));
        assert_eq!(result, Ok(()));
        assert_eq!(harness.main_window.shows(), 1);
        assert_eq!(
            harness.events.events(),
            [AppEvent::OnboardingRequested(OnboardingRequested {})]
        );
    }
}
