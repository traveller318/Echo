/*!
 * SOURCE OF TRUTH KEYWORDS: TrayAction, TrayItemSpec, tray menu entry, tray item id, tray check item
 * WHAT:  TrayAction (what a tray menu item does, with its stable menu id) and TrayItemSpec (one registry entry of
 *        the tray menu: its action, label, whether it is a check item and whether a separator precedes it).
 * WHY:   The tray menu is registry data (registry/tray.rs), so adding a tray action is an entry plus one arm where
 *        app/tray.rs carries it out; the menu id round-trips through `id` / `from_id`, so a click can never name an
 *        action that does not exist.
 * WHERE: registry/tray.rs (TRAY_MENU); app/tray.rs builds the menu and dispatches clicks.
 */

use super::StaticStr;

/// What a tray menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayAction {
    /// Show and focus the main window.
    OpenMain,
    /// Start dictating, or stop the take that is recording (the same machine input as any UI toggle).
    ToggleDictation,
    /// Paste the newest transcript into the app the user was in.
    PasteLast,
    /// Switch Echo's hotkeys off or back on.
    PauseHotkeys,
    /// Quit Echo.
    Quit,
}

impl TrayAction {
    /// Every action, so the registry can prove each has an entry.
    pub const ALL: [Self; 5] = [
        Self::OpenMain,
        Self::ToggleDictation,
        Self::PasteLast,
        Self::PauseHotkeys,
        Self::Quit,
    ];

    /// The menu item id.
    pub const fn id(self) -> &'static str {
        match self {
            Self::OpenMain => "open-main",
            Self::ToggleDictation => "toggle-dictation",
            Self::PasteLast => "paste-last",
            Self::PauseHotkeys => "pause-hotkeys",
            Self::Quit => "quit",
        }
    }

    /// The action a menu item id names.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.id() == id)
    }
}

/// One tray menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItemSpec {
    pub action: TrayAction,
    pub label: StaticStr,
    /// A check item (its tick follows state Echo owns).
    pub check: bool,
    /// A separator line comes before it.
    pub separated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_round_trip() {
        for action in TrayAction::ALL {
            assert_eq!(TrayAction::from_id(action.id()), Some(action));
        }
        assert_eq!(TrayAction::from_id("gone"), None);
    }
}
