/*!
 * SOURCE OF TRUTH KEYWORDS: launch pipeline, reconcile launch at startup, sync_at_startup, after_settings_change, origin_from_args, main_window_at_launch, start in tray, Task Manager switch-off respected
 * WHAT:  Startup behaviour: `origin_from_args` tells a start at sign-in from a user's launch; `main_window_at_launch`
 *        decides whether the main window shows; `sync_at_startup` brings Windows' start-at-sign-in entry and
 *        `general.launch_at_startup` into agreement once per launch; `after_settings_change` registers or removes
 *        the entry when the user flips the setting.
 * WHY:   Windows is the truth for whether Echo starts at sign-in, because the user can switch it off in Task
 *        Manager without Echo hearing of it. At startup: setting on and no entry → register (first run, or an entry a
 *        cleaner removed); an entry switched off in Task Manager → the setting follows it (stored as internal state, so
 *        no effect re-registers it); an entry for another executable path → rewritten; setting off → the entry goes.
 *        Only the user's own flip in Echo overrides Task Manager. The main window starts hidden only when Windows
 *        started Echo at sign-in with `general.start_minimized` on, and never while onboarding is due, so a first run
 *        always shows its setup (05 §6 item resolved in step 25) and a double-click always shows the window. Nothing
 *        here fails startup: a registry error is logged and the setting simply applies next time.
 * WHERE: app/mod.rs (origin, window at launch), app/bootstrap (`sync_at_startup` once the settings are resolved),
 *        pipeline/settings_effects.rs (`after_settings_change`).
 */

use crate::{
    pipeline::settings_store,
    ports::{EventSink, LaunchAtLogin},
    registry::{self, settings::keys},
    services::Db,
    types::{
        AppEvent, LaunchAtLoginState, LaunchOrigin, MainWindowAtLaunch, SettingValue,
        SettingsSnapshot, SharedSettings,
    },
};

/// Who started this process, from its command-line arguments (the program name included or not).
pub fn origin_from_args<I, S>(args: I) -> LaunchOrigin
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if args
        .into_iter()
        .any(|arg| arg.as_ref() == registry::launch::LOGIN_ARG)
    {
        LaunchOrigin::Login
    } else {
        LaunchOrigin::User
    }
}

/// Whether the main window shows at launch; `onboarding_due` is `pipeline::onboarding::needs(..).required()`.
pub fn main_window_at_launch(
    settings: &SettingsSnapshot,
    origin: LaunchOrigin,
    onboarding_due: bool,
) -> MainWindowAtLaunch {
    let hidden = origin == LaunchOrigin::Login
        && registry::settings::start_minimized(settings)
        && !onboarding_due;
    if hidden {
        MainWindowAtLaunch::StayHidden
    } else {
        MainWindowAtLaunch::Show
    }
}

/// What `sync_at_startup` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchSync {
    /// Windows and the setting already agreed.
    InSync,
    Registered,
    Unregistered,
    /// Task Manager had switched Echo off, so the setting was turned off to match.
    FollowedTaskManager,
    /// This build cannot start at sign-in, or Windows could not be read or written (logged).
    Skipped,
}

/**
 * SOURCE OF TRUTH KEYWORDS: sync_at_startup, launch at startup reconcile, first run registration, repair stale Run entry
 * WHAT:  Brings the start-at-sign-in entry and `general.launch_at_startup` into agreement (rules in the file header);
 *        reports what it did.
 * WHY:   Runs once per launch, after the settings are resolved and before the windows exist, so a first run is
 *        registered without the user doing anything and the Settings toggle opens showing the truth.
 * WHERE: app/bootstrap `start`.
 */
pub fn sync_at_startup(
    launch: &dyn LaunchAtLogin,
    settings: &SharedSettings,
    db: &Db,
    events: &dyn EventSink<AppEvent>,
) -> LaunchSync {
    if !launch.caps().available {
        return LaunchSync::Skipped;
    }
    let wanted = registry::settings::launch_at_startup(&settings.current());
    let state = match launch.state() {
        Ok(state) => state,
        Err(error) => {
            tracing::warn!(
                detail = error.detail(),
                "the start-at-sign-in entry could not be read"
            );
            return LaunchSync::Skipped;
        }
    };
    let outcome = match (wanted, state) {
        (true, LaunchAtLoginState::NotRegistered)
        | (
            true,
            LaunchAtLoginState::Registered {
                approved: true,
                current: false,
            },
        ) => launch.register().map(|()| LaunchSync::Registered),
        (
            true,
            LaunchAtLoginState::Registered {
                approved: false, ..
            },
        ) => settings_store::store_internal(
            settings,
            db,
            events,
            &keys::LAUNCH_AT_STARTUP,
            SettingValue::Bool(false),
        )
        .map(|()| LaunchSync::FollowedTaskManager),
        (false, LaunchAtLoginState::Registered { .. }) => {
            launch.unregister().map(|()| LaunchSync::Unregistered)
        }
        (true, LaunchAtLoginState::Registered { .. })
        | (false, LaunchAtLoginState::NotRegistered) => Ok(LaunchSync::InSync),
    };
    match outcome {
        Ok(done) => {
            if done != LaunchSync::InSync {
                tracing::info!(?done, "start at sign-in brought in line");
            }
            done
        }
        Err(error) => {
            tracing::warn!(
                detail = error.detail(),
                "the start-at-sign-in entry could not be updated"
            );
            LaunchSync::Skipped
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: launch at startup setting change, register on toggle, unregister on toggle
 * WHAT:  When a write changed `general.launch_at_startup`, registers (on) or removes (off) the entry.
 * WHY:   The toggle takes effect at once, and turning it on is the user's explicit choice, so it also clears a Task
 *        Manager switch-off (the port's `register`). A failure is logged; the next launch's sync retries.
 * WHERE: pipeline/settings_effects.rs, after every settings write.
 */
pub fn after_settings_change(
    launch: &dyn LaunchAtLogin,
    before: &SettingsSnapshot,
    after: &SettingsSnapshot,
) {
    let wanted = registry::settings::launch_at_startup(after);
    if !launch.caps().available || registry::settings::launch_at_startup(before) == wanted {
        return;
    }
    let result = if wanted {
        launch.register()
    } else {
        launch.unregister()
    };
    if let Err(error) = result {
        tracing::warn!(
            wanted,
            detail = error.detail(),
            "the start-at-sign-in entry could not be changed"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        ports::fakes::{FakeLaunchAtLogin, LaunchAtLoginCall, RecordingSink},
        registry::settings::{self, resolve},
        types::{AppError, SettingsChanged},
    };

    fn with_setting(key: &crate::types::SettingKey, on: bool) -> SettingsSnapshot {
        resolve([(key.clone(), SettingValue::Bool(on))])
    }

    struct Rig {
        settings: SharedSettings,
        db: Db,
        events: Arc<RecordingSink<AppEvent>>,
    }

    fn rig(launch_at_startup: bool) -> Rig {
        let db = Db::open_in_memory().unwrap();
        crate::services::settings::set::set(
            &db,
            &keys::LAUNCH_AT_STARTUP,
            &SettingValue::Bool(launch_at_startup),
        )
        .unwrap();
        Rig {
            settings: SharedSettings::new(settings_store::resolved(&db).unwrap()),
            db,
            events: Arc::default(),
        }
    }

    fn sync(rig: &Rig, launch: &FakeLaunchAtLogin) -> LaunchSync {
        sync_at_startup(launch, &rig.settings, &rig.db, rig.events.as_ref())
    }

    #[test]
    fn the_login_argument_marks_a_start_at_sign_in() {
        assert_eq!(
            origin_from_args(["echo.exe", "--minimized"]),
            LaunchOrigin::Login
        );
        assert_eq!(origin_from_args(["echo.exe"]), LaunchOrigin::User);
        assert_eq!(origin_from_args(Vec::<String>::new()), LaunchOrigin::User);
    }

    #[test]
    fn only_a_start_at_sign_in_with_start_minimized_and_no_setup_due_stays_hidden() {
        let minimized = with_setting(&keys::START_MINIMIZED, true);
        let shown = with_setting(&keys::START_MINIMIZED, false);
        assert_eq!(
            main_window_at_launch(&minimized, LaunchOrigin::Login, false),
            MainWindowAtLaunch::StayHidden
        );
        assert_eq!(
            main_window_at_launch(&minimized, LaunchOrigin::Login, true),
            MainWindowAtLaunch::Show,
            "a first run never hides its setup"
        );
        assert_eq!(
            main_window_at_launch(&minimized, LaunchOrigin::User, false),
            MainWindowAtLaunch::Show,
            "a double-click always shows the window"
        );
        assert_eq!(
            main_window_at_launch(&shown, LaunchOrigin::Login, false),
            MainWindowAtLaunch::Show
        );
        assert_eq!(
            main_window_at_launch(&settings::defaults(), LaunchOrigin::Login, false),
            MainWindowAtLaunch::StayHidden,
            "02 §3.3: start in the tray by default"
        );
    }

    #[test]
    fn a_first_run_registers_and_a_stale_entry_is_repaired() {
        let rig = rig(true);
        let fresh = FakeLaunchAtLogin::new();
        assert_eq!(sync(&rig, &fresh), LaunchSync::Registered);
        assert!(fresh.current().starts_at_login());
        assert_eq!(sync(&rig, &fresh), LaunchSync::InSync);

        let moved = FakeLaunchAtLogin::in_state(LaunchAtLoginState::Registered {
            approved: true,
            current: false,
        });
        assert_eq!(sync(&rig, &moved), LaunchSync::Registered);
        assert_eq!(moved.calls(), [LaunchAtLoginCall::Register]);
    }

    #[test]
    fn a_task_manager_switch_off_is_respected_and_mirrored_in_the_setting() {
        let rig = rig(true);
        let switched_off = FakeLaunchAtLogin::in_state(LaunchAtLoginState::Registered {
            approved: false,
            current: true,
        });
        assert_eq!(sync(&rig, &switched_off), LaunchSync::FollowedTaskManager);
        assert!(
            switched_off.calls().is_empty(),
            "Echo never re-enables it by itself"
        );
        assert!(!registry::settings::launch_at_startup(
            &rig.settings.current()
        ));
        assert!(rig.events.events().iter().any(|event| matches!(
            event,
            AppEvent::SettingsChanged(SettingsChanged { key, value: SettingValue::Bool(false) })
                if *key == keys::LAUNCH_AT_STARTUP
        )));
    }

    #[test]
    fn the_setting_off_removes_the_entry_and_an_unavailable_build_changes_nothing() {
        let rig = rig(false);
        let registered = FakeLaunchAtLogin::in_state(LaunchAtLoginState::Registered {
            approved: true,
            current: true,
        });
        assert_eq!(sync(&rig, &registered), LaunchSync::Unregistered);
        assert_eq!(registered.current(), LaunchAtLoginState::NotRegistered);

        let dev = FakeLaunchAtLogin::unavailable();
        assert_eq!(sync(&self::rig(true), &dev), LaunchSync::Skipped);
        assert!(dev.calls().is_empty());

        let broken = FakeLaunchAtLogin::new();
        broken.fail_next(AppError::Internal.into());
        assert_eq!(sync(&self::rig(true), &broken), LaunchSync::Skipped);
    }

    #[test]
    fn flipping_the_setting_registers_or_removes_the_entry() {
        let launch = FakeLaunchAtLogin::in_state(LaunchAtLoginState::Registered {
            approved: false,
            current: true,
        });
        let on = with_setting(&keys::LAUNCH_AT_STARTUP, true);
        let off = with_setting(&keys::LAUNCH_AT_STARTUP, false);
        after_settings_change(&launch, &on, &on);
        assert!(launch.calls().is_empty(), "no change, nothing to do");
        after_settings_change(&launch, &off, &on);
        assert!(
            launch.current().starts_at_login(),
            "the user's own choice clears Task Manager's"
        );
        after_settings_change(&launch, &on, &off);
        assert_eq!(
            launch.calls(),
            [LaunchAtLoginCall::Register, LaunchAtLoginCall::Unregister]
        );
        let dev = FakeLaunchAtLogin::unavailable();
        after_settings_change(&dev, &off, &on);
        assert!(dev.calls().is_empty());
    }
}
