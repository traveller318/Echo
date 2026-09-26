/*!
 * SOURCE OF TRUTH KEYWORDS: updates pipeline, update check, install update, check_at_startup, StartupCheck, automatic update check, no install during take, updater caps guard
 * WHAT:  The rules around the Updater port: `check` asks the update source (or answers `not_configured` without
 *        asking when there is none), `install` installs what a check found unless a take is in progress, and
 *        `check_at_startup` runs the automatic check once after startup and toasts a found update.
 * WHY:   02 §11: this build has no update source, so the caps guard here keeps an unavailable updater from ever being
 *        asked (no update network call), whatever adapter is plugged in. Installing replaces the running app, so it is
 *        refused with `Busy` while a take is between its hotkey press and its settled result; the take's audio is on
 *        disk either way (02 §7.3), so a take started after this check is still recovered if the installer closes
 *        Echo. The automatic check honours, in order, the caps, `updates.auto_check` and offline mode (the registry's
 *        Network permission, the same answer the command factory gives), so a disabled or offline build does nothing;
 *        a failed check is logged and shown nowhere, because nobody asked for it.
 * WHERE: ipc/commands/updates.rs (`updates_check`, `updates_install`); app/bootstrap (`schedule_update_check` runs
 *        `check_at_startup` AUTO_CHECK_DELAY after the windows exist).
 */

use crate::{
    ports::{Notifier, PrivacyConsent, Updater},
    registry::{self, permissions::PermissionCtx},
    types::{
        AppError, Permission, PortError, PortResult, ResourceKind, SessionStatus, SettingsSnapshot,
        StartupCheck, UpdateStatus,
    },
};

/// Asks the update source for a newer Echo; `NotConfigured` without asking anything when there is no source.
pub async fn check(updater: &dyn Updater) -> PortResult<UpdateStatus> {
    if !updater.caps().available {
        return Ok(UpdateStatus::NotConfigured);
    }
    updater.check().await
}

/// Installs the update a check found; `Busy` while `session` has a take in progress, `NotFound { update }` when
/// there is no update source.
pub async fn install(updater: &dyn Updater, session: SessionStatus) -> PortResult<()> {
    if !updater.caps().available {
        return Err(AppError::NotFound {
            resource: ResourceKind::Update,
        }
        .into());
    }
    if session.is_in_progress() {
        return Err(PortError::new(AppError::Busy)
            .with_detail("an update cannot be installed while a take is in progress"));
    }
    updater.install().await
}

/// What the automatic check reads and reports through.
pub struct StartupCheckDeps<'a> {
    pub updater: &'a dyn Updater,
    /// The settings in effect when the check runs.
    pub settings: &'a SettingsSnapshot,
    pub consent: &'a dyn PrivacyConsent,
    pub notifier: &'a dyn Notifier,
}

/**
 * SOURCE OF TRUTH KEYWORDS: check_at_startup, automatic update check, update available toast, auto_check setting, offline skips update check
 * WHAT:  Runs the automatic update check once: skipped when there is no source, when `updates.auto_check` is off or
 *        when offline mode is on; otherwise asks the source and toasts a newer version. Reports what it did.
 * WHY:   Rules in the file header. A failed toast is only logged: the user can still find the update in About.
 * WHERE: app/bootstrap `schedule_update_check`.
 */
pub async fn check_at_startup(deps: StartupCheckDeps<'_>) -> StartupCheck {
    if !deps.updater.caps().available {
        return StartupCheck::Unavailable;
    }
    if !registry::settings::auto_check_updates(deps.settings) {
        return StartupCheck::Off;
    }
    let network = registry::permissions::check(
        Permission::Network,
        &PermissionCtx {
            settings: deps.settings,
            consent: deps.consent,
        },
    );
    match network {
        Ok(state) if state.is_granted() => {}
        Ok(_) => return StartupCheck::Offline,
        Err(error) => {
            tracing::warn!(
                detail = error.detail(),
                "the network permission is unknown; skipping the update check"
            );
            return StartupCheck::Offline;
        }
    }
    let status = match deps.updater.check().await {
        Ok(status) => status,
        Err(error) => {
            tracing::warn!(
                code = error.error().code().as_str(),
                detail = error.detail(),
                "the automatic update check failed"
            );
            return StartupCheck::Failed;
        }
    };
    if let UpdateStatus::Available { version, .. } = &status {
        tracing::info!(%version, "a newer Echo is available");
        let toast = registry::updates::update_available_toast(version);
        if let Err(error) = deps.notifier.toast(&toast) {
            tracing::warn!(
                detail = error.detail(),
                "the update toast could not be shown"
            );
        }
    }
    StartupCheck::Checked(status)
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::{
        ports::fakes::{FakeNotifier, FakePrivacyConsent, FakeUpdater, poll_once},
        registry::settings::keys,
        types::SettingValue,
    };

    fn online() -> SettingsSnapshot {
        registry::settings::resolve([(keys::OFFLINE_MODE, SettingValue::Bool(false))])
    }

    fn startup(
        updater: &FakeUpdater,
        settings: &SettingsSnapshot,
        notifier: &FakeNotifier,
    ) -> StartupCheck {
        let consent = FakePrivacyConsent::granted();
        let Poll::Ready(outcome) = poll_once(check_at_startup(StartupCheckDeps {
            updater,
            settings,
            consent: &consent,
            notifier,
        })) else {
            panic!("the startup check did not finish");
        };
        outcome
    }

    #[test]
    fn an_unavailable_updater_is_never_asked() {
        let updater = FakeUpdater::disabled();
        assert_eq!(
            poll_once(check(&updater)),
            Poll::Ready(Ok(UpdateStatus::NotConfigured))
        );
        let Poll::Ready(Err(error)) = poll_once(install(&updater, SessionStatus::Idle)) else {
            panic!("installing without an update source must fail");
        };
        assert_eq!(
            error.into_app_error(),
            AppError::NotFound {
                resource: ResourceKind::Update
            }
        );
        let notifier = FakeNotifier::default();
        assert_eq!(
            startup(&updater, &online(), &notifier),
            StartupCheck::Unavailable
        );
        assert_eq!(updater.checks(), 0, "no update network call");
        assert!(notifier.toasts().is_empty());
    }

    #[test]
    fn check_asks_an_available_source() {
        let updater = FakeUpdater::offering("0.2.0");
        assert!(matches!(
            poll_once(check(&updater)),
            Poll::Ready(Ok(UpdateStatus::Available { .. }))
        ));
        assert_eq!(updater.checks(), 1);
    }

    #[test]
    fn install_waits_for_the_take_to_settle() {
        let updater = FakeUpdater::offering("0.2.0");
        for status in [
            SessionStatus::Arming,
            SessionStatus::Recording,
            SessionStatus::CancelPending,
            SessionStatus::Finalizing,
            SessionStatus::Delivering,
        ] {
            let Poll::Ready(Err(error)) = poll_once(install(&updater, status)) else {
                panic!("installing during {status:?} must be refused");
            };
            assert_eq!(error.into_app_error(), AppError::Busy);
        }
        assert_eq!(updater.installs(), 0);
        assert_eq!(
            poll_once(install(&updater, SessionStatus::Idle)),
            Poll::Ready(Ok(()))
        );
        assert_eq!(updater.installs(), 1);
    }

    #[test]
    fn the_automatic_check_toasts_a_found_update() {
        let updater = FakeUpdater::offering("0.2.0");
        let notifier = FakeNotifier::default();
        assert!(matches!(
            startup(&updater, &online(), &notifier),
            StartupCheck::Checked(UpdateStatus::Available { .. })
        ));
        assert_eq!(
            notifier.toasts(),
            [registry::updates::update_available_toast("0.2.0")]
        );
    }

    #[test]
    fn the_automatic_check_respects_the_setting_and_offline_mode() {
        let updater = FakeUpdater::offering("0.2.0");
        let notifier = FakeNotifier::default();
        let off = registry::settings::resolve([
            (keys::OFFLINE_MODE, SettingValue::Bool(false)),
            (keys::UPDATES_AUTO_CHECK, SettingValue::Bool(false)),
        ]);
        assert_eq!(startup(&updater, &off, &notifier), StartupCheck::Off);
        let offline = registry::settings::resolve([(keys::OFFLINE_MODE, SettingValue::Bool(true))]);
        assert_eq!(
            startup(&updater, &offline, &notifier),
            StartupCheck::Offline
        );
        assert_eq!(updater.checks(), 0);
        assert!(notifier.toasts().is_empty());
    }

    #[test]
    fn a_failed_check_or_toast_stays_quiet() {
        let updater = FakeUpdater::offering("0.2.0");
        let notifier = FakeNotifier::default();
        updater.fail_next_check(PortError::new(AppError::Network).with_detail("unreachable"));
        assert_eq!(
            startup(&updater, &online(), &notifier),
            StartupCheck::Failed
        );
        assert!(notifier.toasts().is_empty());

        notifier.fail_next(PortError::new(AppError::Internal).with_detail("no toast"));
        assert!(matches!(
            startup(&updater, &online(), &notifier),
            StartupCheck::Checked(UpdateStatus::Available { .. })
        ));
    }
}
