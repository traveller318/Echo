/*!
 * SOURCE OF TRUTH KEYWORDS: find setting spec, resolve settings, defaults, SettingsSnapshot builder
 * WHAT:  `find` (the spec of a key), `resolve` (stored rows over defaults → SettingsSnapshot) and `defaults`.
 * WHY:   The one builder of a SettingsSnapshot, so a stored value that no longer validates never reaches the core.
 * WHERE: Re-exported by registry/settings; called by app/bootstrap, ipc/commands/settings and tests.
 */

use super::SETTINGS;
use crate::types::{SettingKey, SettingSpec, SettingValue, SettingsSnapshot};

/// The spec of `key`.
pub fn find(key: &SettingKey) -> Option<&'static SettingSpec> {
    SETTINGS.iter().find(|spec| spec.key == *key)
}

/**
 * SOURCE OF TRUTH KEYWORDS: resolve settings, stored over default, settings overlay, drop invalid stored value
 * WHAT:  Builds the SettingsSnapshot: every registry default, overlaid by each stored value that still passes its
 *        spec's kind check.
 * WHY:   Stored rows can outlive the spec that wrote them (a key removed, bounds tightened, a kind changed); such a
 *        value falls back to the default instead of reaching the pipeline. Unknown keys are ignored. Membership in
 *        runtime options is not checked here: an engine id whose adapter is gone is handled where the engine is
 *        built (NotFound → default engine), so resolving never needs the installed-engine state.
 * WHERE: Settings reads in commands and the session actor (after services/settings returns the stored rows);
 *        `defaults()`.
 */
pub fn resolve(stored: impl IntoIterator<Item = (SettingKey, SettingValue)>) -> SettingsSnapshot {
    let mut values: Vec<(SettingKey, SettingValue)> = SETTINGS
        .iter()
        .map(|spec| (spec.key.clone(), spec.default.clone()))
        .collect();
    for (key, value) in stored {
        let Some(spec) = find(&key) else { continue };
        if spec.validate(&value).is_err() {
            continue;
        }
        if let Some(slot) = values.iter_mut().find(|(existing, _)| *existing == key) {
            slot.1 = value;
        }
    }
    SettingsSnapshot::from_resolved(values)
}

/// Every setting at its default value.
pub fn defaults() -> SettingsSnapshot {
    resolve(std::iter::empty())
}
