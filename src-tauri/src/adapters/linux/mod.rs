/*!
 * SOURCE OF TRUTH KEYWORDS: Linux helpers, display server, X11 or Wayland, DisplayServer::detect, XDG_SESSION_TYPE, WAYLAND_DISPLAY, spawn detached, xdg-open
 * WHAT:  What the Linux adapters share: which display server the session runs (`DisplayServer`) and `spawn_detached`,
 *        which starts a desktop helper (xdg-open, a settings app) without waiting for it or inheriting its output.
 * WHY:   X11 and Wayland need different adapters for hotkeys, typing, the focused window and the pill, and the
 *        session type is read once from the environment the desktop gives every app. Echo's own windows may run
 *        under XWayland (GDK_BACKEND=x11, see app/mod.rs), so `DISPLAY` alone does not mean an X11 session:
 *        Wayland wins whenever the session says so. A helper is reaped on its own thread so it never lingers as a
 *        zombie.
 * WHERE: The `linux.rs` adapters and app/bootstrap.rs (adapter choice); the Windows build has adapters/win32.
 */

mod desktop_entry;
pub mod keymap;
mod portal;
mod process_group;
mod wake_fd;
pub mod x11;

pub use portal::register_app;
pub use process_group::KillOnCloseGroup;
pub use wake_fd::{Wake, WakeFd};

use std::{
    ffi::OsStr,
    process::{Command, Stdio},
    thread,
};

use crate::types::{AppError, PortError, PortResult};

/// The display server of the user's session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    X11,
    Wayland,
}

impl DisplayServer {
    /// Reads the session type the desktop exported; a session that says nothing is X11 when DISPLAY is set.
    pub fn detect() -> Self {
        Self::from_env(
            std::env::var_os("XDG_SESSION_TYPE").as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
        )
    }

    fn from_env(session_type: Option<&OsStr>, wayland_display: bool) -> Self {
        match session_type.and_then(OsStr::to_str) {
            Some(kind) if kind.eq_ignore_ascii_case("wayland") => Self::Wayland,
            Some(kind) if kind.eq_ignore_ascii_case("x11") => Self::X11,
            _ if wayland_display => Self::Wayland,
            _ => Self::X11,
        }
    }
}

/// Starts `program` with `args` in the background; Ok once it is running.
pub fn spawn_detached<I, S>(program: &str, args: I) -> PortResult<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            PortError::new(AppError::Internal)
                .with_detail(format!("could not start {program}: {error}"))
        })?;
    // Reaped on its own thread, so the helper never lingers as a zombie.
    let reaper = thread::Builder::new()
        .name("echo-helper-reaper".to_owned())
        .spawn(move || drop(child.wait()));
    if let Err(error) = reaper {
        tracing::debug!(%error, program, "helper reaper did not start");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[test]
    fn the_session_type_decides_and_wayland_display_breaks_ties() {
        let wayland = OsString::from("wayland");
        let x11 = OsString::from("x11");
        let tty = OsString::from("tty");
        assert_eq!(
            DisplayServer::from_env(Some(&wayland), false),
            DisplayServer::Wayland
        );
        assert_eq!(
            DisplayServer::from_env(Some(&x11), true),
            DisplayServer::X11
        );
        assert_eq!(
            DisplayServer::from_env(Some(&tty), true),
            DisplayServer::Wayland
        );
        assert_eq!(DisplayServer::from_env(None, false), DisplayServer::X11);
    }
}
