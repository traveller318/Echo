/*!
 * SOURCE OF TRUTH KEYWORDS: PillLook, PillVisibility, PillStyle, pill settings view, pill always on screen, pill only while recording, pill style, movable pill, pill_get_look
 * WHAT:  How the user wants the pill: when it is on screen (PillVisibility, `pill.visibility`), how it looks
 *        (PillStyle, `pill.style`) and whether it can be dragged (`pill.movable`), bundled as PillLook for the pill
 *        page.
 * WHY:   Rust owns settings and pushes them as typed events (root CLAUDE.md §7), so the pill page reads one view
 *        (`pill_get_look`, then PillLookChanged) instead of knowing setting keys. The enums' strings are also the
 *        stored option values (registry/settings), so the setting and the view cannot drift, like ThemePreference.
 *        Where the pill was dragged to is not part of the view: only Rust places the window.
 * WHERE: Built by `registry::settings::pill_look`; read by pipeline/pill.rs (PillPresenter: show while idle, drag),
 *        returned by `pill_get_look` (ipc/commands/pill.rs) and carried by PillLookChanged; rendered by src/pill.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

/// The `pill.visibility` choice: when the pill is on screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PillVisibility {
    /// Always on screen; between takes it rests in its idle look.
    Always,
    /// Only while a take runs (and while it reports how the take ended).
    #[default]
    Recording,
}

impl PillVisibility {
    pub const ALL: [Self; 2] = [Self::Always, Self::Recording];

    /// The stored `pill.visibility` value (and wire value) of this choice.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Recording => "recording",
        }
    }

    /// The choice a stored `pill.visibility` value names, if any.
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.as_str() == value)
    }
}

/// The `pill.style` choice: how the pill looks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PillStyle {
    /// The full pill: logo, waveform, ✕ and stop while recording.
    #[default]
    Full,
    /// A compact pill with Echo's colour logo: a round badge at rest, logo, waveform and stop while recording.
    Icon,
    /// The compact pill in grey: grey logo and a neutral waveform.
    Mono,
}

impl PillStyle {
    pub const ALL: [Self; 3] = [Self::Full, Self::Icon, Self::Mono];

    /// The stored `pill.style` value (and wire value) of this choice.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Icon => "icon",
            Self::Mono => "mono",
        }
    }

    /// The choice a stored `pill.style` value names, if any.
    pub fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.as_str() == value)
    }
}

/// Everything the pill page needs to know about the user's pill settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PillLook {
    pub visibility: PillVisibility,
    pub style: PillStyle,
    /// The pill can be dragged anywhere; off keeps it at the bottom centre.
    pub movable: bool,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn values_round_trip_and_match_the_wire() {
        for visibility in PillVisibility::ALL {
            assert_eq!(
                PillVisibility::from_value(visibility.as_str()),
                Some(visibility)
            );
            assert_eq!(
                serde_json::to_value(visibility).unwrap(),
                json!(visibility.as_str())
            );
        }
        for style in PillStyle::ALL {
            assert_eq!(PillStyle::from_value(style.as_str()), Some(style));
            assert_eq!(serde_json::to_value(style).unwrap(), json!(style.as_str()));
        }
        assert_eq!(PillVisibility::from_value("hidden"), None);
        assert_eq!(PillStyle::from_value("neon"), None);
    }

    #[test]
    fn the_default_look_is_todays_pill() {
        assert_eq!(
            serde_json::to_value(PillLook::default()).unwrap(),
            json!({ "visibility": "recording", "style": "full", "movable": false })
        );
    }
}
