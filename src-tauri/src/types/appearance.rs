/*!
 * SOURCE OF TRUTH KEYWORDS: AppearanceView, ThemePreference, Transparency, Backdrop, appearance, data-theme, data-transparency, data-backdrop, Mica, general.theme
 * WHAT:  The appearance facts Rust owns and the UI paints from: the theme the user picked (ThemePreference, the
 *        `general.theme` setting), whether Windows "Transparency effects" are on (Transparency) and what sits
 *        behind the main window (Backdrop), bundled as AppearanceView.
 * WHY:   Rust owns domain state and pushes it as typed events (root CLAUDE.md §7), so both windows receive one
 *        view instead of each combining a setting, a Windows registry value and an OS version check. The wire
 *        values are exactly the `<html>` attribute values of docs/04 §2 (`data-theme`, `data-transparency`,
 *        `data-backdrop`), so the UI copies them without a mapping table. ThemePreference's strings are also the
 *        `general.theme` option values (registry/settings), so the setting and the view cannot drift.
 * WHERE: Built by pipeline/appearance.rs; returned by `appearance_get` (ipc/commands/system.rs) and carried by
 *        the AppearanceChanged event; applied natively by app/windows.rs and in the UI by src/lib/appearance.ts.
 *        Transparency is what the SystemAppearance port reports.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

/// The `general.theme` choice: follow Windows, or always light or dark.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    /// The stored `general.theme` value (and wire value) of this choice.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// The choice a stored `general.theme` value names, if any.
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.as_str() == value)
    }
}

/// Whether Windows "Transparency effects" are on (05 W17).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Transparency {
    #[default]
    Full,
    /// Transparency effects are off: every glass tint switches to its solid value.
    Reduced,
}

/// What is behind the main window's content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Backdrop {
    /// Native Mica (Windows 11 with transparency on); the page paints no background of its own.
    Mica,
    /// No native material: the page paints `--color-bg`.
    #[default]
    Solid,
}

/// Everything the UI needs to theme both windows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AppearanceView {
    pub theme: ThemePreference,
    pub transparency: Transparency,
    /// The main window's backdrop; the pill is always transparent with CSS glass (05 W16).
    pub backdrop: Backdrop,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn theme_values_round_trip_and_match_the_wire() {
        for theme in ThemePreference::ALL {
            assert_eq!(ThemePreference::from_value(theme.as_str()), Some(theme));
            assert_eq!(serde_json::to_value(theme).unwrap(), json!(theme.as_str()));
        }
        assert_eq!(ThemePreference::from_value("sepia"), None);
    }

    #[test]
    fn view_serializes_to_the_html_attribute_values() {
        let view = AppearanceView {
            theme: ThemePreference::Dark,
            transparency: Transparency::Reduced,
            backdrop: Backdrop::Mica,
        };
        assert_eq!(
            serde_json::to_value(view).unwrap(),
            json!({ "theme": "dark", "transparency": "reduced", "backdrop": "mica" })
        );
    }
}
