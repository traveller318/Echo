/*!
 * SOURCE OF TRUTH KEYWORDS: updates registry, AUTO_CHECK_DELAY, update_available_toast, update policy, automatic update check, update source swap
 * WHAT:  How Echo treats updates, as data: how long after startup the automatic check waits (AUTO_CHECK_DELAY) and the
 *        toast a found update raises (`update_available_toast`).
 * WHY:   This build ships with no update source (02 §11): the Updater adapter declares `available: false`, so the
 *        automatic check never runs and nothing goes online. The policy still lives here so a future source (for
 *        example GitHub Releases once the repo is public) is a new Updater adapter chosen in app/bootstrap, its host
 *        in registry/network.rs and its endpoint in tauri.conf.json, with no change to the pipeline, the commands or
 *        the UI. The check waits well past STARTUP_IDLE_DELAY so it never competes with the speech engine's load
 *        (05 W19). Toast copy is calm and never contains transcript text (types/notification.rs).
 * WHERE: app/bootstrap (`schedule_update_check` sleeps AUTO_CHECK_DELAY); pipeline/updates.rs
 *        (`check_at_startup` shows `update_available_toast`).
 */

use std::time::Duration;

use crate::types::{StaticStr, Toast, ToastKind};

/// How long after the windows exist the automatic update check runs (only when the updater is available, the
/// setting is on and offline mode is off).
pub const AUTO_CHECK_DELAY: Duration = Duration::from_secs(60);

/// The toast the automatic check shows when a newer Echo is available.
pub fn update_available_toast(version: &str) -> Toast {
    Toast {
        kind: ToastKind::Info,
        title: StaticStr::new("Update available"),
        body: StaticStr::from(format!(
            "Echo {version} is ready. To install it, open Settings, About and choose Check for updates."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::launch::STARTUP_IDLE_DELAY;

    #[test]
    fn the_automatic_check_waits_until_the_warm_up_is_long_done() {
        assert!(AUTO_CHECK_DELAY > STARTUP_IDLE_DELAY * 10);
    }

    #[test]
    fn the_toast_names_the_version_and_where_to_install_it() {
        let toast = update_available_toast("0.2.0");
        assert_eq!(toast.kind, ToastKind::Info);
        assert!(toast.body.contains("0.2.0"));
        assert!(toast.body.contains("Settings"));
        assert!(toast.body.ends_with('.'));
    }
}
