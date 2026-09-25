/*!
 * SOURCE OF TRUTH KEYWORDS: settings effects, SettingsEffects, apply settings live, after settings write, derived settings state, live settings change
 * WHAT:  SettingsEffects: the one place that makes a stored settings change take effect in the running app.
 *        `apply(before, after)` compares the snapshot a write replaced with the new one and runs every consequence:
 *        the appearance (AppearanceChanged), the retention sweep, the speech engine swap and the dashboard refresh
 *        (MetricsChanged).
 * WHY:   Every setting works live or says a restart is needed (step 18); most are simply read at the moment they
 *        matter (per take, per delivery, per command) and need nothing here. The rest derive running state, and each
 *        owner decides from `before` and `after` whether it is affected, so no code matches on a setting key
 *        (root CLAUDE.md §3). One hook keeps `settings_set` and `settings_reset` identical, and a later feature that
 *        follows a setting (sound cues, autostart, the LLM sidecar, updates) adds one line here instead of touching
 *        the commands. Hotkeys are not here: a new combination must bind before the write is stored, so the command
 *        does it and can refuse the write (pipeline/hotkeys.rs `rebind_setting`).
 * WHERE: ipc/commands/settings.rs after every successful write or reset, with the parts CommandCtx holds.
 */

use crate::{
    pipeline::{appearance, asr, asr::AsrWorker, retention, retention::RetentionHandle},
    ports::{EventSink, SystemAppearance},
    registry,
    types::{AppEvent, AppPaths, AppearanceChanged, MetricsChanged, SettingsSnapshot},
};

/// What a settings change may act on.
pub struct SettingsEffects<'a> {
    /// The Windows appearance switches, to build the full AppearanceView.
    pub appearance: &'a dyn SystemAppearance,
    /// Wakes the retention sweeper.
    pub retention: &'a RetentionHandle,
    /// The speech engine owner, for an engine or accelerator change.
    pub asr: &'a AsrWorker,
    /// Where engine models live.
    pub paths: &'a AppPaths,
    /// Rust → UI events.
    pub events: &'a dyn EventSink<AppEvent>,
}

impl SettingsEffects<'_> {
    /// Applies everything that changed between `before` and `after`; settings that are read when used need nothing.
    pub fn apply(&self, before: &SettingsSnapshot, after: &SettingsSnapshot) {
        if let Some(view) = appearance::after_settings_change(before, after, self.appearance) {
            self.events.emit(AppearanceChanged(view).into());
        }
        if retention::policy_changed(before, after) {
            // A lower limit frees the disk now, not at the next daily sweep.
            self.retention.sweep_soon();
        }
        asr::reload_on_change(self.asr, before, after, self.paths);
        if registry::metrics::inputs_changed(before, after) {
            self.events.emit(MetricsChanged {}.into());
        }
    }
}
