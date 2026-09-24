/*!
 * SOURCE OF TRUTH KEYWORDS: hotkey registry, HOTKEYS, record hotkey, paste last hotkey, cancel Esc, default accelerator, hotkey scope, effective shortcut
 * WHAT:  Every global hotkey Echo binds (record, paste-last, cancel) with its default combination, the setting
 *        that rebinds it and its scope; typed id constants; and the combination in effect for given settings.
 * WHY:   A hotkey is a registry entry (02 §3.3): the session reacts to a HotkeyId, never to key text. The default
 *        combinations are declared once here and reused as the settings defaults (registry/settings), so the two
 *        cannot drift. Cancel is fixed to Esc and scoped to a session so Esc is never taken from other apps
 *        outside a take (05 W10). `Escape` is the accelerator spelling the global-shortcut adapter parses.
 * WHERE: Read by the pipeline's hotkey wiring (register Always entries at startup, DuringSession entries on
 *        RecordPressed), the session actor (which id fired) and registry/settings (defaults).
 */

use super::settings::keys;
use crate::types::{HotkeyId, HotkeyScope, HotkeySpec, SettingsSnapshot, Shortcut, StaticStr};

pub const RECORD: HotkeyId = HotkeyId::from_static("record");
pub const PASTE_LAST: HotkeyId = HotkeyId::from_static("paste-last");
pub const CANCEL: HotkeyId = HotkeyId::from_static("cancel");

/// Default combinations (05 decision log: rarely bound by other apps).
pub const RECORD_DEFAULT: &str = "Ctrl+Alt+Space";
pub const PASTE_LAST_DEFAULT: &str = "Ctrl+Alt+V";
pub const CANCEL_DEFAULT: &str = "Escape";

/// Every hotkey.
pub const HOTKEYS: &[HotkeySpec] = &[
    HotkeySpec {
        id: RECORD,
        label: StaticStr::new("Start or stop dictation"),
        default_accelerator: Shortcut::from_static(RECORD_DEFAULT),
        setting_key: Some(keys::RECORD_HOTKEY),
        scope: HotkeyScope::Always,
    },
    HotkeySpec {
        id: PASTE_LAST,
        label: StaticStr::new("Paste last transcript"),
        default_accelerator: Shortcut::from_static(PASTE_LAST_DEFAULT),
        setting_key: Some(keys::PASTE_LAST_HOTKEY),
        scope: HotkeyScope::Always,
    },
    HotkeySpec {
        id: CANCEL,
        label: StaticStr::new("Cancel take"),
        default_accelerator: Shortcut::from_static(CANCEL_DEFAULT),
        setting_key: None,
        scope: HotkeyScope::DuringSession,
    },
];

/// The hotkey `id`.
pub fn find(id: &HotkeyId) -> Option<&'static HotkeySpec> {
    HOTKEYS.iter().find(|spec| spec.id == *id)
}

/// Every hotkey registered with `scope`.
pub fn in_scope(scope: HotkeyScope) -> impl Iterator<Item = &'static HotkeySpec> {
    HOTKEYS.iter().filter(move |spec| spec.scope == scope)
}

/// The combination `spec` is bound to: its setting's value, or the default when it has no setting.
pub fn effective_shortcut(spec: &HotkeySpec, settings: &SettingsSnapshot) -> Shortcut {
    spec.setting_key
        .as_ref()
        .and_then(|key| settings.hotkey(key))
        .map_or_else(
            || spec.default_accelerator.clone(),
            |combination| Shortcut::from(combination.to_owned()),
        )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        registry::{settings, tests::is_registry_id},
        types::{SettingKind, SettingValue},
    };

    #[test]
    fn ids_are_unique_kebab_case() {
        let mut ids = HashSet::new();
        for spec in HOTKEYS {
            assert!(is_registry_id(spec.id.as_str()), "{}", spec.id);
            assert!(ids.insert(spec.id.as_str()), "duplicate hotkey {}", spec.id);
        }
        assert_eq!(ids, HashSet::from(["record", "paste-last", "cancel"]));
    }

    #[test]
    fn rebindable_hotkeys_match_their_setting() {
        for spec in HOTKEYS {
            let Some(key) = &spec.setting_key else {
                continue;
            };
            let setting = settings::find(key).unwrap();
            assert_eq!(setting.kind, SettingKind::Hotkey, "{key}");
            assert_eq!(
                setting.default,
                SettingValue::Hotkey(StaticStr::from(spec.default_accelerator.to_string())),
                "{} default differs from {key}",
                spec.id
            );
        }
    }

    #[test]
    fn only_cancel_waits_for_a_session() {
        let scoped: Vec<&str> = in_scope(HotkeyScope::DuringSession)
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(scoped, ["cancel"]);
        assert_eq!(in_scope(HotkeyScope::Always).count(), 2);
        assert_eq!(
            find(&CANCEL).map(|spec| spec.setting_key.is_none()),
            Some(true)
        );
    }

    #[test]
    fn effective_shortcut_follows_the_setting() {
        let record = find(&RECORD).unwrap();
        let cancel = find(&CANCEL).unwrap();
        let defaults = settings::defaults();
        assert_eq!(
            effective_shortcut(record, &defaults),
            Shortcut::from_static(RECORD_DEFAULT)
        );
        let rebound = settings::resolve([(
            settings::keys::RECORD_HOTKEY,
            SettingValue::Hotkey(StaticStr::new("Ctrl+Shift+D")),
        )]);
        assert_eq!(
            effective_shortcut(record, &rebound),
            Shortcut::from_static("Ctrl+Shift+D")
        );
        assert_eq!(
            effective_shortcut(cancel, &rebound),
            Shortcut::from_static(CANCEL_DEFAULT)
        );
    }
}
