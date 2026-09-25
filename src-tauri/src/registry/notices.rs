/*!
 * SOURCE OF TRUTH KEYWORDS: one-time notices, NOTICES, BLUETOOTH_MIC_NOTICE, Bluetooth microphone hint, show once, hidden setting flag
 * WHAT:  Every toast Echo shows only once per installation, each paired with the hidden Bool setting that remembers
 *        it was shown.
 * WHY:   A first-time hint is data: a new one is an entry here plus its hidden setting (registry/settings), and
 *        pipeline/notices.rs shows any of them the same way. The Bluetooth hint exists because a Bluetooth headset
 *        switches to its hands-free profile when the microphone opens, which cuts the first 0.5–2 s (05 W11); it is
 *        advice, never a block, so it is a calm info toast. Copy never contains transcript text (types/notification.rs).
 * WHERE: The session runner shows BLUETOOTH_MIC_NOTICE when a take opens a Bluetooth microphone.
 */

use super::settings::keys;
use crate::types::{OneTimeNotice, StaticStr, Toast, ToastKind};

/// A take opened a Bluetooth microphone for the first time (05 W11).
pub const BLUETOOTH_MIC_NOTICE: OneTimeNotice = OneTimeNotice {
    shown: keys::BLUETOOTH_HINT_SHOWN,
    toast: Toast {
        kind: ToastKind::Info,
        title: StaticStr::new("Bluetooth microphone"),
        body: StaticStr::new(
            "The first second of each recording may be cut while your headset switches modes. Pause briefly before you speak.",
        ),
    },
};

/// Every one-time notice.
pub const NOTICES: &[OneTimeNotice] = &[BLUETOOTH_MIC_NOTICE];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        registry::settings,
        types::{SettingKind, SettingValue},
    };

    #[test]
    fn every_notice_is_remembered_by_its_own_hidden_flag() {
        let mut flags = HashSet::new();
        for notice in NOTICES {
            assert!(
                flags.insert(notice.shown.as_str()),
                "{} is shared",
                notice.shown
            );
            let spec = settings::find(&notice.shown).unwrap();
            assert_eq!(spec.kind, SettingKind::Bool, "{}", notice.shown);
            assert_eq!(spec.default, SettingValue::Bool(false), "{}", notice.shown);
            assert!(
                !spec.visible,
                "{} must not appear in Settings",
                notice.shown
            );
            assert!(!notice.toast.title.trim().is_empty());
            assert!(notice.toast.body.ends_with('.'));
        }
        let defaults = settings::defaults();
        assert!(!settings::notice_shown(&defaults, &BLUETOOTH_MIC_NOTICE));
        let shown = settings::resolve([(keys::BLUETOOTH_HINT_SHOWN, SettingValue::Bool(true))]);
        assert!(settings::notice_shown(&shown, &BLUETOOTH_MIC_NOTICE));
    }
}
