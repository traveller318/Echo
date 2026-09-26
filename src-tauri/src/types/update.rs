/*!
 * SOURCE OF TRUTH KEYWORDS: UpdateStatus, update check, NotConfigured, UpToDate, update available, DisabledUpdater, StartupCheck, automatic update check outcome
 * WHAT:  UpdateStatus: the answer to "is there a newer Echo?": no update source is configured, already current,
 *        or a newer version is available. StartupCheck: what the automatic check did (never crosses IPC).
 * WHY:   Updates ship disabled (02 §11): the `DisabledUpdater` answers `not_configured` without any network call,
 *        and the UI hides update controls from `UpdaterCaps.available`. A future update source is a new adapter
 *        returning the other variants; nothing else changes.
 * WHERE: `Updater::check` (ports/updater.rs); returned to the UI by `updates_check`. StartupCheck is returned by
 *        pipeline/updates.rs `check_at_startup` and logged by app/bootstrap.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

/// Result of an update check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateStatus {
    /// No update source exists in this build; nothing was checked.
    NotConfigured,
    UpToDate,
    Available {
        /// SemVer of the newer release.
        version: String,
        /// Release notes, when the source provides them.
        notes: Option<String>,
    },
}

/// What the automatic update check did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupCheck {
    /// This build has no update source; nothing was asked.
    Unavailable,
    /// `updates.auto_check` is off.
    Off,
    /// Offline mode is on, or the network permission could not be read (logged).
    Offline,
    /// The source answered (a found update was toasted).
    Checked(UpdateStatus),
    /// The source could not be asked (logged).
    Failed,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn statuses_are_tagged_by_kind() {
        assert_eq!(
            serde_json::to_value(UpdateStatus::NotConfigured).unwrap(),
            json!({ "kind": "not_configured" })
        );
        assert_eq!(
            serde_json::to_value(UpdateStatus::Available {
                version: String::from("0.2.0"),
                notes: None,
            })
            .unwrap(),
            json!({ "kind": "available", "version": "0.2.0", "notes": null })
        );
    }
}
