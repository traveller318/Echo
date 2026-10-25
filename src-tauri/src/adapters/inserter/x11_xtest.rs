/*!
 * SOURCE OF TRUTH KEYWORDS: X11XTestInserter, XTest fake input, synthetic Ctrl+V X11, terminal paste Ctrl+Shift+V, modifier release wait, focus restore, _NET_ACTIVE_WINDOW activate
 * WHAT:  X11XTestInserter: TextInserter that pastes what the clipboard holds into the take's target window with a
 *        synthetic Ctrl+V (Ctrl+Shift+V in a terminal) sent through the XTest extension. Caps: uses the clipboard,
 *        reaches any window.
 * WHY:   The same job as win32_send_input.rs, with the same hazards. The user usually still holds the dictation
 *        chord when the take stops, so a Ctrl+V sent at once would arrive as Ctrl+Alt+V: the paste polls the real
 *        keyboard state until every modifier is up (at most MODIFIER_WAIT), then sends key-ups for any still held, so
 *        the synthetic keys meet a clean modifier state. The paste goes to the window the take started in: if focus
 *        moved it is asked back to that window, and when that does not happen in time the call fails rather than
 *        pasting into whatever is in front now (Delivery then copies and tells the user). Terminals do not treat
 *        Ctrl+V as paste (it is a literal in many shells), so a target whose executable is a known terminal gets
 *        Ctrl+Shift+V. XTest events are indistinguishable from real ones, which is also why no permission is
 *        involved on X11 (any X client can do this). `text` is unused: it is already on the clipboard.
 * WHERE: Built by app/platform.rs on Linux for an X11 session; called through `dyn TextInserter` by pipeline/delivery.rs.
 */

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use x11rb::{connection::Connection, protocol::xtest::ConnectionExt as _};

use crate::{
    adapters::linux::{keymap::x11_keycode_from_vk, x11::X11Session},
    ports::TextInserter,
    types::{AppError, AppTarget, InserterCaps, Permission, PortError, PortResult},
};

/// How long the paste waits for the dictation chord's modifiers to come up.
const MODIFIER_WAIT: Duration = Duration::from_millis(400);

/// How long a window gets to become active again after being asked.
const FOCUS_WAIT: Duration = Duration::from_millis(400);

/// Pause between synthetic events, so slow clients see each edge.
const STEP: Duration = Duration::from_millis(8);

/// Poll interval for the waits above.
const POLL: Duration = Duration::from_millis(10);

/// Executables (the kernel's `comm`, at most 15 characters) of terminals, where paste is Ctrl+Shift+V.
const TERMINALS: &[&str] = &[
    "alacritty",
    "foot",
    "footclient",
    "gnome-terminal-",
    "gnome-terminal",
    "kitty",
    "konsole",
    "lxterminal",
    "mate-terminal",
    "qterminal",
    "st",
    "terminator",
    "terminology",
    "tilix",
    "urxvt",
    "wezterm-gui",
    "xfce4-terminal",
    "xterm",
    "ptyxis",
    "ghostty",
    "blackbox",
    "kgx",
];

const VK_CONTROL: u16 = 0xA2;
const VK_SHIFT: u16 = 0xA0;
const VK_V: u16 = b'V' as u16;

const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;

pub struct X11XTestInserter {
    session: Arc<X11Session>,
}

impl X11XTestInserter {
    pub fn new(session: Arc<X11Session>) -> Self {
        Self { session }
    }

    fn fake(&self, kind: u8, vk: u16) -> PortResult<()> {
        let keycode = x11_keycode_from_vk(vk)
            .and_then(|code| u8::try_from(code).ok())
            .ok_or_else(|| failure("a key has no X11 keycode"))?;
        self.session
            .connection()
            .xtest_fake_input(kind, keycode, 0, self.session.root(), 0, 0, 0)
            .map_err(|error| failure(&error.to_string()))?;
        Ok(())
    }

    /// Waits for the modifiers to come up, then lets go of any that stay down.
    fn clear_modifiers(&self) -> PortResult<()> {
        let started = Instant::now();
        let mut held = self.session.modifiers_down();
        while !held.is_empty() && started.elapsed() < MODIFIER_WAIT {
            thread::sleep(POLL);
            held = self.session.modifiers_down();
        }
        for vk in held {
            self.fake(KEY_RELEASE, vk)?;
        }
        Ok(())
    }

    /// Brings the take's window back to the front; fails when it does not get there.
    fn focus(&self, target: &AppTarget) -> PortResult<()> {
        let Ok(window) = u32::try_from(target.window.raw()) else {
            return Ok(());
        };
        if window == 0 || self.session.active_window() == Some(window) {
            return Ok(());
        }
        self.session.activate(window)?;
        let started = Instant::now();
        while started.elapsed() < FOCUS_WAIT {
            if self.session.active_window() == Some(window) {
                return Ok(());
            }
            thread::sleep(POLL);
        }
        Err(failure(
            "the window the take started in could not be focused",
        ))
    }
}

impl TextInserter for X11XTestInserter {
    fn caps(&self) -> InserterCaps {
        InserterCaps {
            can_target_elevated: true,
            uses_clipboard: true,
        }
    }

    fn insert(&self, target: &AppTarget, _text: &str) -> PortResult<()> {
        self.clear_modifiers()?;
        self.focus(target)?;
        let shift = is_terminal(target.exe_name.as_deref());
        self.fake(KEY_PRESS, VK_CONTROL)?;
        if shift {
            self.fake(KEY_PRESS, VK_SHIFT)?;
        }
        self.fake(KEY_PRESS, VK_V)?;
        self.session
            .connection()
            .flush()
            .map_err(|e| failure(&e.to_string()))?;
        thread::sleep(STEP);
        self.fake(KEY_RELEASE, VK_V)?;
        if shift {
            self.fake(KEY_RELEASE, VK_SHIFT)?;
        }
        self.fake(KEY_RELEASE, VK_CONTROL)?;
        self.session
            .connection()
            .flush()
            .map_err(|e| failure(&e.to_string()))?;
        // The client reads the clipboard after it gets the key; let the events land before Delivery moves on.
        thread::sleep(STEP);
        Ok(())
    }
}

fn is_terminal(exe: Option<&str>) -> bool {
    exe.is_some_and(|name| {
        TERMINALS
            .iter()
            .any(|terminal| name.eq_ignore_ascii_case(terminal))
    })
}

fn failure(detail: &str) -> PortError {
    PortError::new(AppError::PermissionDenied {
        permission: Permission::InputInjection,
    })
    .with_detail(format!("X11 paste failed: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_paste_with_shift_and_other_apps_do_not() {
        assert!(is_terminal(Some("kitty")));
        assert!(is_terminal(Some("Konsole")));
        assert!(is_terminal(Some("gnome-terminal-")));
        assert!(!is_terminal(Some("firefox")));
        assert!(!is_terminal(Some("code")));
        assert!(!is_terminal(None));
    }

    #[test]
    fn every_typed_key_has_an_x11_keycode() {
        for vk in [VK_CONTROL, VK_SHIFT, VK_V] {
            assert!(
                x11_keycode_from_vk(vk)
                    .and_then(|c| u8::try_from(c).ok())
                    .is_some()
            );
        }
    }
}
