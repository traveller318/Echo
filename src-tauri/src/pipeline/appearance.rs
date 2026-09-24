/*!
 * SOURCE OF TRUTH KEYWORDS: appearance view, AppearanceRelay, compose appearance, backdrop rule, transparency fallback, AppearanceChanged emit
 * WHAT:  Builds the AppearanceView from the settings and the SystemAppearance port (`view`, `current`), decides
 *        whether a settings write changed it (`after_settings_change`), and
 *        AppearanceRelay: the sink the port pushes transparency changes into, which re-emits the full view as
 *        AppearanceChanged.
 * WHY:   The rule "Mica only when the OS has it and transparency is on" (04 §2) lives in one place, so the
 *        command, the relay and the native window setup cannot disagree. An unreadable transparency switch is
 *        logged and treated as `reduced`: the solid tints always meet contrast (04 §7), so a failed read never
 *        makes text unreadable. The relay emits the whole view, never a partial change, so the UI copies it
 *        without merging (root CLAUDE.md §7).
 * WHERE: `current` is called by ipc/commands/system.rs (`appearance_get`), ipc/commands/settings.rs (to announce
 *        a theme change) and app/windows.rs (startup); AppearanceRelay is wired by app/bootstrap into
 *        `SystemAppearance::listen`.
 */

use std::sync::Arc;

use crate::{
    ports::{EventSink, SystemAppearance},
    registry,
    types::{
        AppEvent, AppearanceCaps, AppearanceChanged, AppearanceView, Backdrop, SettingsSnapshot,
        SharedSettings, Transparency,
    },
};

/// The view for these settings, OS caps and transparency switch.
pub fn view(
    settings: &SettingsSnapshot,
    caps: AppearanceCaps,
    transparency: Transparency,
) -> AppearanceView {
    let backdrop = if caps.mica && transparency == Transparency::Full {
        Backdrop::Mica
    } else {
        Backdrop::Solid
    };
    AppearanceView {
        theme: registry::settings::theme(settings),
        transparency,
        backdrop,
    }
}

/// The view right now, reading the transparency switch from the port.
pub fn current(settings: &SettingsSnapshot, system: &dyn SystemAppearance) -> AppearanceView {
    let transparency = system.transparency().unwrap_or_else(|error| {
        tracing::warn!(
            code = error.error().code().as_str(),
            detail = error.detail(),
            "transparency could not be read; using solid tints"
        );
        Transparency::Reduced
    });
    view(settings, system.caps(), transparency)
}

/// The view to announce after a settings write, or `None` when the write did not change the appearance.
pub fn after_settings_change(
    before: &SettingsSnapshot,
    after: &SettingsSnapshot,
    system: &dyn SystemAppearance,
) -> Option<AppearanceView> {
    (registry::settings::theme(before) != registry::settings::theme(after))
        .then(|| current(after, system))
}

/// Re-emits every transparency change the port reports as a full AppearanceChanged.
pub struct AppearanceRelay {
    settings: SharedSettings,
    caps: AppearanceCaps,
    events: Arc<dyn EventSink<AppEvent>>,
}

impl AppearanceRelay {
    pub fn new(
        settings: SharedSettings,
        caps: AppearanceCaps,
        events: Arc<dyn EventSink<AppEvent>>,
    ) -> Self {
        Self {
            settings,
            caps,
            events,
        }
    }
}

impl EventSink<Transparency> for AppearanceRelay {
    fn emit(&self, transparency: Transparency) {
        let view = view(&self.settings.current(), self.caps, transparency);
        self.events.emit(AppearanceChanged(view).into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::{FakeSystemAppearance, RecordingSink},
        registry::settings::keys,
        types::{AppError, PortError, SettingValue, StaticStr, ThemePreference},
    };

    const MICA: AppearanceCaps = AppearanceCaps { mica: true };
    const NO_MICA: AppearanceCaps = AppearanceCaps { mica: false };

    fn dark() -> SettingsSnapshot {
        registry::settings::resolve([(keys::THEME, SettingValue::Enum(StaticStr::new("dark")))])
    }

    #[test]
    fn mica_needs_both_os_support_and_full_transparency() {
        let defaults = registry::settings::defaults();
        assert_eq!(
            view(&defaults, MICA, Transparency::Full).backdrop,
            Backdrop::Mica
        );
        assert_eq!(
            view(&defaults, MICA, Transparency::Reduced).backdrop,
            Backdrop::Solid
        );
        assert_eq!(
            view(&defaults, NO_MICA, Transparency::Full).backdrop,
            Backdrop::Solid
        );
    }

    #[test]
    fn the_theme_comes_from_the_setting() {
        assert_eq!(
            view(&registry::settings::defaults(), MICA, Transparency::Full).theme,
            ThemePreference::System
        );
        assert_eq!(
            view(&dark(), MICA, Transparency::Full).theme,
            ThemePreference::Dark
        );
    }

    #[test]
    fn an_unreadable_switch_falls_back_to_solid_tints() {
        let system = FakeSystemAppearance::mica();
        system.fail_next(PortError::new(AppError::Internal).with_detail("registry locked"));
        let view = current(&registry::settings::defaults(), &system);
        assert_eq!(view.transparency, Transparency::Reduced);
        assert_eq!(view.backdrop, Backdrop::Solid);
        assert_eq!(
            current(&registry::settings::defaults(), &system).backdrop,
            Backdrop::Mica
        );
    }

    #[test]
    fn only_a_theme_change_is_an_appearance_change() {
        let system = FakeSystemAppearance::mica();
        let defaults = registry::settings::defaults();
        let wpm = registry::settings::resolve([(keys::TYPING_WPM, SettingValue::Int(70))]);
        assert_eq!(after_settings_change(&defaults, &wpm, &system), None);
        let changed = after_settings_change(&defaults, &dark(), &system).unwrap();
        assert_eq!(changed.theme, ThemePreference::Dark);
        assert_eq!(changed.backdrop, Backdrop::Mica);
    }

    #[test]
    fn the_relay_emits_the_full_view_with_the_live_settings() {
        let settings = SharedSettings::new(registry::settings::defaults());
        let events = Arc::new(RecordingSink::default());
        let system = FakeSystemAppearance::mica();
        system
            .listen(Arc::new(AppearanceRelay::new(
                settings.clone(),
                system.caps(),
                events.clone(),
            )))
            .unwrap();
        settings.replace(dark());
        assert!(system.switch(Transparency::Reduced));
        assert_eq!(
            events.events(),
            [AppEvent::AppearanceChanged(AppearanceChanged(
                AppearanceView {
                    theme: ThemePreference::Dark,
                    transparency: Transparency::Reduced,
                    backdrop: Backdrop::Solid,
                }
            ))]
        );
    }
}
