/*!
 * SOURCE OF TRUTH KEYWORDS: X11ForegroundApp, active window X11, last external app, PropertyNotify _NET_ACTIVE_WINDOW, window tracker thread, X11 AppTarget
 * WHAT:  X11ForegroundApp: ForegroundApp on EWMH. `current` reads the active window and its facts; `last_external`
 *        answers with the last active window that was not one of Echo's own, which a tracker thread keeps by watching
 *        `_NET_ACTIVE_WINDOW` on the root window.
 * WHY:   Mirrors the Windows adapter (foreground/win32.rs + tracker.rs): the target is read once when a take starts
 *        and rides with it, and a take started from the tray (where Echo's own window or the menu may be active)
 *        pastes into the app the user came from. The tracker waits on the X connection and a wake fd, so it sleeps
 *        until the active window changes and stops at once on drop. A tracker that cannot start is logged and
 *        `last_external` then answers the active window if it is not Echo's, else None (the text is copied). Wayland
 *        has no such lookup; UnknownFocusForeground serves there.
 * WHERE: Built by app/platform.rs on Linux for an X11 session; read by the session actor's Arm and paste-last.
 */

use std::{
    os::fd::AsRawFd,
    sync::Arc,
    thread::{self, JoinHandle},
};

use parking_lot::Mutex;
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{ChangeWindowAttributesAux, ConnectionExt as _, EventMask},
    },
};

use crate::{
    adapters::linux::{Wake, WakeFd, x11::X11Session},
    ports::ForegroundApp,
    types::{AppTarget, PortResult},
};

pub struct X11ForegroundApp {
    session: Arc<X11Session>,
    last_external: Arc<Mutex<Option<AppTarget>>>,
    tracker: Option<Tracker>,
}

struct Tracker {
    wake: Arc<WakeFd>,
    thread: Option<JoinHandle<()>>,
}

impl X11ForegroundApp {
    pub fn new(session: Arc<X11Session>) -> Self {
        let last_external = Arc::new(Mutex::new(None));
        let tracker = Tracker::start(Arc::clone(&session), Arc::clone(&last_external))
            .map_err(|detail| {
                tracing::warn!(detail, "the last app outside Echo cannot be followed")
            })
            .ok();
        Self {
            session,
            last_external,
            tracker,
        }
    }
}

impl Tracker {
    fn start(
        session: Arc<X11Session>,
        last: Arc<Mutex<Option<AppTarget>>>,
    ) -> Result<Self, String> {
        session
            .connection()
            .change_window_attributes(
                session.root(),
                &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )
            .map_err(|error| error.to_string())?
            .check()
            .map_err(|error| error.to_string())?;
        remember(&session, &last);
        let wake = Arc::new(WakeFd::new().map_err(|error| error.to_string())?);
        let thread_wake = Arc::clone(&wake);
        let thread = thread::Builder::new()
            .name("echo-x11-focus".to_owned())
            .spawn(move || watch(&session, &thread_wake, &last))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            wake,
            thread: Some(thread),
        })
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        self.wake.signal();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("the X11 focus thread panicked");
        }
    }
}

/// Records the active window as the last external one unless it is Echo's.
fn remember(session: &X11Session, last: &Mutex<Option<AppTarget>>) {
    let Some(window) = session.active_window() else {
        return;
    };
    if session.is_own(window) {
        return;
    }
    if let Some(target) = session.target_of(window) {
        *last.lock() = Some(target);
    }
}

fn watch(session: &X11Session, wake: &WakeFd, last: &Mutex<Option<AppTarget>>) {
    let conn = session.connection();
    let fd = conn.stream().as_raw_fd();
    loop {
        match wake.wait(fd, -1) {
            Wake::Stop | Wake::Failed => return,
            Wake::Data | Wake::Timeout => {}
        }
        let mut changed = false;
        loop {
            match conn.poll_for_event() {
                Ok(Some(Event::PropertyNotify(_))) => changed = true,
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => return,
            }
        }
        if changed {
            remember(session, last);
        }
    }
}

impl ForegroundApp for X11ForegroundApp {
    fn current(&self) -> PortResult<Option<AppTarget>> {
        Ok(self
            .session
            .active_window()
            .and_then(|window| self.session.target_of(window)))
    }

    fn last_external(&self) -> PortResult<Option<AppTarget>> {
        if self.tracker.is_none() {
            return Ok(self
                .session
                .active_window()
                .filter(|window| !self.session.is_own(*window))
                .and_then(|window| self.session.target_of(window)));
        }
        Ok(self.last_external.lock().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs an X server (DISPLAY): an X11 or XWayland session has one.
    #[test]
    fn a_live_session_answers_and_stops_cleanly() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let Ok(session) = X11Session::connect() else {
            return;
        };
        let app = X11ForegroundApp::new(Arc::new(session));
        let _ = app.current();
        let _ = app.last_external();
        drop(app);
    }
}
