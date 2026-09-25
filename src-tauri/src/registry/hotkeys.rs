/*!
 * SOURCE OF TRUTH KEYWORDS: hotkey registry, HOTKEYS, record hotkey, paste last hotkey, cancel Esc, default accelerator, hotkey scope, effective shortcut, for_setting, conflicting
 * WHAT:  Every global hotkey Echo binds (record, paste-last, cancel) with its default combination, the setting
 *        that rebinds it, its scope and its action; typed id constants; the combination in effect for given settings;
 *        which hotkey a setting rebinds (`for_setting`); and which other hotkey a new combination would collide with
 *        (`conflicting`).
 * WHY:   A hotkey is a registry entry (02 §3.3): the session reacts to the HotkeyAction of the HotkeyId that fired,
 *        never to key text or to an id. The default combinations are declared once here and reused as the settings
 *        defaults (registry/settings), so the two cannot drift. Cancel is fixed to Esc and scoped to a session so Esc
 *        is never taken from other apps outside a take (05 W10). `Escape` is the accelerator spelling the hotkey
 *        adapter parses; the record default is the modifier-only chord Ctrl+Alt, held to talk.
 * WHERE: Read by pipeline/hotkeys.rs (register Always entries once the session actor is ready, DuringSession
 *        entries while a take records, rebind after a hotkey setting changes), the session actor (the action of the
 *        id that fired) and registry/settings (defaults).
 */

use std::collections::BTreeSet;

use super::settings::keys;
use crate::types::{
    HotkeyAction, HotkeyId, HotkeyScope, HotkeySpec, SettingKey, SettingsSnapshot, Shortcut,
    StaticStr,
};

pub const RECORD: HotkeyId = HotkeyId::from_static("record");
pub const PASTE_LAST: HotkeyId = HotkeyId::from_static("paste-last");
pub const CANCEL: HotkeyId = HotkeyId::from_static("cancel");

/// Default combinations (05 decision log): hold Ctrl+Alt to dictate (a modifier-only chord the keyboard hook binds),
/// Ctrl+Alt+V to paste the last transcript again.
pub const RECORD_DEFAULT: &str = "Ctrl+Alt";
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
        action: HotkeyAction::Record,
    },
    HotkeySpec {
        id: PASTE_LAST,
        label: StaticStr::new("Paste last transcript"),
        default_accelerator: Shortcut::from_static(PASTE_LAST_DEFAULT),
        setting_key: Some(keys::PASTE_LAST_HOTKEY),
        scope: HotkeyScope::Always,
        action: HotkeyAction::PasteLast,
    },
    HotkeySpec {
        id: CANCEL,
        label: StaticStr::new("Cancel take"),
        default_accelerator: Shortcut::from_static(CANCEL_DEFAULT),
        setting_key: None,
        scope: HotkeyScope::DuringSession,
        action: HotkeyAction::CancelTake,
    },
];

/// The hotkey `id`.
pub fn find(id: &HotkeyId) -> Option<&'static HotkeySpec> {
    HOTKEYS.iter().find(|spec| spec.id == *id)
}

/// What the hotkey `id` does; None for an id no entry has (a stale event after a registry change).
pub fn action_of(id: &HotkeyId) -> Option<HotkeyAction> {
    find(id).map(|spec| spec.action)
}

/// The hotkey `key` rebinds; None for a setting that is not a hotkey.
pub fn for_setting(key: &SettingKey) -> Option<&'static HotkeySpec> {
    HOTKEYS
        .iter()
        .find(|spec| spec.setting_key.as_ref() == Some(key))
}

/// Every hotkey registered with `scope`.
pub fn in_scope(scope: HotkeyScope) -> impl Iterator<Item = &'static HotkeySpec> {
    HOTKEYS.iter().filter(move |spec| spec.scope == scope)
}

/**
 * SOURCE OF TRUTH KEYWORDS: hotkey conflict check, conflicting hotkey, same chord, duplicate Echo hotkey, Hotkey conflict
 * WHAT:  The other Echo hotkey that already uses the combination `shortcut`, if any, when `shortcut` is written to
 *        the hotkey setting `key` (settings in effect: `settings`).
 * WHY:   Two Echo hotkeys on one chord are refused before either is stored (02 §9, `Hotkey{conflict}`). The adapter
 *        reports it as well, but only among hotkeys it has bound, so a hotkey that is not bound yet (Esc outside a
 *        take, one the session does not handle yet) would collide later. The comparison ignores key order, case and
 *        spacing around `+`; a finer equivalence stays the adapter's, which still answers when it registers.
 * WHERE: registry::settings::validate, for writes to a Hotkey setting.
 */
pub fn conflicting(
    key: &SettingKey,
    shortcut: &str,
    settings: &SettingsSnapshot,
) -> Option<&'static HotkeySpec> {
    let wanted = chord_keys(shortcut);
    HOTKEYS.iter().find(|spec| {
        spec.setting_key.as_ref() != Some(key)
            && chord_keys(effective_shortcut(spec, settings).as_str()) == wanted
    })
}

/// The keys of an accelerator, order- and case-insensitive.
fn chord_keys(shortcut: &str) -> BTreeSet<String> {
    shortcut
        .split('+')
        .map(|part| part.trim().to_lowercase())
        .filter(|part| !part.is_empty())
        .collect()
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
    fn settings_lead_back_to_their_hotkey() {
        assert_eq!(
            for_setting(&keys::RECORD_HOTKEY).map(|spec| &spec.id),
            Some(&RECORD)
        );
        assert_eq!(
            for_setting(&keys::PASTE_LAST_HOTKEY).map(|spec| &spec.id),
            Some(&PASTE_LAST)
        );
        assert!(for_setting(&keys::HOTKEY_MODE).is_none());
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
    fn every_action_has_exactly_one_hotkey() {
        assert_eq!(action_of(&RECORD), Some(HotkeyAction::Record));
        assert_eq!(action_of(&PASTE_LAST), Some(HotkeyAction::PasteLast));
        assert_eq!(action_of(&CANCEL), Some(HotkeyAction::CancelTake));
        assert_eq!(action_of(&HotkeyId::from_static("unknown")), None);
        let actions: HashSet<_> = HOTKEYS.iter().map(|spec| spec.action).collect();
        assert_eq!(actions.len(), HOTKEYS.len());
    }

    #[test]
    fn a_combination_another_hotkey_uses_is_a_conflict() {
        let defaults = settings::defaults();
        let paste_last = |shortcut: &str| {
            conflicting(&keys::PASTE_LAST_HOTKEY, shortcut, &defaults).map(|spec| &spec.id)
        };
        assert_eq!(paste_last("Ctrl+Alt"), Some(&RECORD));
        assert_eq!(paste_last(" alt + CTRL "), Some(&RECORD), "order and case");
        assert_eq!(
            paste_last("Escape"),
            Some(&CANCEL),
            "the session-scoped Esc"
        );
        assert_eq!(paste_last("Ctrl+Alt+V"), None, "its own combination");
        assert_eq!(paste_last("Ctrl+Shift+V"), None);
        let rebound = settings::resolve([(
            settings::keys::RECORD_HOTKEY,
            SettingValue::Hotkey(StaticStr::new("Ctrl+Shift+D")),
        )]);
        assert_eq!(
            conflicting(&keys::PASTE_LAST_HOTKEY, "Ctrl+Alt", &rebound),
            None,
            "the record hotkey moved away"
        );
        assert_eq!(
            conflicting(&keys::PASTE_LAST_HOTKEY, "Shift+Ctrl+D", &rebound).map(|spec| &spec.id),
            Some(&RECORD)
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
