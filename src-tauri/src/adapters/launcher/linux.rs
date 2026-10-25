/*!
 * SOURCE OF TRUTH KEYWORDS: XdgLauncher, xdg-open, open folder Linux, sound settings page, gnome-control-center, systemsettings
 * WHAT:  XdgLauncher: SystemLauncher that opens folders with `xdg-open` and the microphone settings in the desktop's
 *        own settings app (GNOME's sound panel, KDE's audio module), falling back to the first one installed.
 * WHY:   xdg-open is the freedesktop way to open a path in the user's file manager on any desktop. Linux has no
 *        microphone privacy page, so `MicrophonePrivacy` opens the input-device settings, where a muted or missing
 *        microphone is fixed; the desktop is read from XDG_CURRENT_DESKTOP.
 * WHERE: Built by app/bootstrap on Linux into CommandCtx.
 */

use std::path::Path;

use crate::{
    adapters::linux::spawn_detached,
    ports::SystemLauncher,
    types::{AppError, PortError, PortResult, SettingsPage},
};

/// Settings apps that open a sound/input page, tried in this order after the current desktop's own.
const SOUND_SETTINGS: &[(&str, &str, &[&str])] = &[
    ("GNOME", "gnome-control-center", &["sound"]),
    ("KDE", "systemsettings", &["kcm_pulseaudio"]),
    ("XFCE", "pavucontrol", &["--tab=4"]),
    ("", "pavucontrol", &["--tab=4"]),
];

#[derive(Debug, Default)]
pub struct XdgLauncher;

impl XdgLauncher {
    pub const fn new() -> Self {
        Self
    }
}

impl SystemLauncher for XdgLauncher {
    fn open_folder(&self, dir: &Path) -> PortResult<()> {
        spawn_detached("xdg-open", [dir])
    }

    fn open_settings_page(&self, page: SettingsPage) -> PortResult<()> {
        match page {
            SettingsPage::MicrophonePrivacy => open_sound_settings(),
        }
    }
}

fn open_sound_settings() -> PortResult<()> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let current = |name: &str| {
        !name.is_empty()
            && desktop
                .split(':')
                .any(|part| part.eq_ignore_ascii_case(name))
    };
    let mut tried = Vec::new();
    let ordered = SOUND_SETTINGS
        .iter()
        .filter(|(name, ..)| current(name))
        .chain(SOUND_SETTINGS.iter());
    for (_, program, args) in ordered {
        if tried.contains(program) {
            continue;
        }
        tried.push(*program);
        if spawn_detached(program, *args).is_ok() {
            return Ok(());
        }
    }
    Err(PortError::new(AppError::Internal).with_detail(format!(
        "no sound settings app found for desktop `{desktop}`"
    )))
}
