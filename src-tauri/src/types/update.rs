/*!
 * SOURCE OF TRUTH KEYWORDS: UpdateStatus, update check, NotConfigured, UpToDate, update available, DisabledUpdater
 * WHAT:  UpdateStatus: the answer to "is there a newer Echo?": no update source is configured, already current,
 *        or a newer version is available.
 * WHY:   Updates ship disabled (02 §11): the `DisabledUpdater` answers `not_configured` without any network call,
 *        and the UI hides update controls from `UpdaterCaps.available`. A future update source is a new adapter
 *        returning the other variants; nothing else changes.
 * WHERE: `Updater::check` (ports/updater.rs); returned to the UI by `updates_check`.
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
