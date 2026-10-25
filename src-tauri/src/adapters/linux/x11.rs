/*!
 * SOURCE OF TRUTH KEYWORDS: X11Session, _NET_ACTIVE_WINDOW, _NET_WM_PID, WM_CLASS, _NET_WORKAREA, RandR monitors, AppTarget from window, modifier keys down, X11 shared connection
 * WHAT:  X11Session: one connection to the X server with the interned atoms Echo needs, and the questions the X11
 *        foreground and paste adapters ask: the active window, the target facts of a window (process id, executable
 *        name, work area of its monitor), which modifier keys are down now, and a request to activate a window.
 * WHY:   Both X11 adapters need the same few EWMH facts; one place reads them so the foreground adapter and the paste
 *        adapter agree on what a "target" is. A window manager that does not publish `_NET_ACTIVE_WINDOW` (rare)
 *        gives no target and Delivery copies. The executable name is `/proc/<pid>/comm` (kernel-truncated to 15
 *        characters, plenty to tell a terminal from an editor); the process id comes from `_NET_WM_PID`, which a
 *        client may omit, so the name is best effort like on Windows. The work area is `_NET_WORKAREA` (the
 *        desktop's usable rectangle, panels excluded) cut to the RandR monitor holding the window's centre, which
 *        places the pill above a panel on a multi-monitor desktop. Nothing here reads a key's meaning, only whether
 *        the modifier keys are down.
 * WHERE: adapters/foreground/x11.rs and adapters/inserter/x11_xtest.rs.
 */

use std::fs;

use x11rb::{
    connection::Connection,
    protocol::{
        randr::ConnectionExt as _,
        xproto::{
            AtomEnum, ClientMessageData, ClientMessageEvent, ConnectionExt as _, EventMask, Window,
        },
    },
    rust_connection::RustConnection,
};

use crate::{
    adapters::linux::keymap::x11_keycode_from_vk,
    types::{AppError, AppTarget, PortError, PortResult, ScreenRect, WindowHandle},
};

/// Virtual keys of the modifiers (both sides) whose state decides when a paste may start.
pub const MODIFIER_VKS: [u16; 8] = [0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x5B, 0x5C];

pub struct X11Session {
    conn: RustConnection,
    root: Window,
    active_window: u32,
    wm_pid: u32,
    workarea: u32,
    own_pid: u32,
}

impl X11Session {
    pub fn connect() -> PortResult<Self> {
        let (conn, screen) = RustConnection::connect(None)
            .map_err(|error| failure(&format!("the X server cannot be reached: {error}")))?;
        let root = conn.setup().roots[screen].root;
        let intern = |name: &str| -> PortResult<u32> {
            conn.intern_atom(false, name.as_bytes())
                .map_err(|error| failure(&error.to_string()))?
                .reply()
                .map(|reply| reply.atom)
                .map_err(|error| failure(&error.to_string()))
        };
        Ok(Self {
            active_window: intern("_NET_ACTIVE_WINDOW")?,
            wm_pid: intern("_NET_WM_PID")?,
            workarea: intern("_NET_WORKAREA")?,
            root,
            own_pid: std::process::id(),
            conn,
        })
    }

    pub fn connection(&self) -> &RustConnection {
        &self.conn
    }

    pub fn root(&self) -> Window {
        self.root
    }

    /// The window the window manager calls active; None when there is none or it publishes none.
    pub fn active_window(&self) -> Option<Window> {
        let reply = self
            .conn
            .get_property(false, self.root, self.active_window, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        reply.value32()?.next().filter(|window| *window != 0)
    }

    /// The facts about `window` that make a paste target; None when it vanished.
    pub fn target_of(&self, window: Window) -> Option<AppTarget> {
        let process_id = self.pid_of(window).unwrap_or(0);
        Some(AppTarget {
            window: WindowHandle::from_raw(isize::try_from(window).ok()?),
            process_id,
            exe_name: (process_id != 0).then(|| comm_of(process_id)).flatten(),
            work_area: self.work_area_of(window),
            elevated: false,
        })
    }

    /// Whether `window` belongs to Echo itself.
    pub fn is_own(&self, window: Window) -> bool {
        self.pid_of(window) == Some(self.own_pid)
    }

    fn pid_of(&self, window: Window) -> Option<u32> {
        let reply = self
            .conn
            .get_property(false, window, self.wm_pid, AtomEnum::CARDINAL, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        reply.value32()?.next().filter(|pid| *pid != 0)
    }

    /// The usable rectangle of the monitor that holds `window`'s centre.
    fn work_area_of(&self, window: Window) -> Option<ScreenRect> {
        let geometry = self.conn.get_geometry(window).ok()?.reply().ok()?;
        let origin = self
            .conn
            .translate_coordinates(window, self.root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        let centre = (
            i32::from(origin.dst_x) + i32::from(geometry.width) / 2,
            i32::from(origin.dst_y) + i32::from(geometry.height) / 2,
        );
        let monitors = self
            .conn
            .randr_get_monitors(self.root, true)
            .ok()?
            .reply()
            .ok()?
            .monitors;
        let monitor = monitors.iter().find(|monitor| {
            centre.0 >= i32::from(monitor.x)
                && centre.0 < i32::from(monitor.x) + i32::from(monitor.width)
                && centre.1 >= i32::from(monitor.y)
                && centre.1 < i32::from(monitor.y) + i32::from(monitor.height)
        })?;
        let monitor_rect = ScreenRect {
            x: i32::from(monitor.x),
            y: i32::from(monitor.y),
            width: u32::from(monitor.width),
            height: u32::from(monitor.height),
        };
        Some(self.desktop_work_area().map_or(monitor_rect, |area| {
            intersect(&monitor_rect, &area).unwrap_or(monitor_rect)
        }))
    }

    /// `_NET_WORKAREA` of the first desktop.
    fn desktop_work_area(&self) -> Option<ScreenRect> {
        let reply = self
            .conn
            .get_property(false, self.root, self.workarea, AtomEnum::CARDINAL, 0, 4)
            .ok()?
            .reply()
            .ok()?;
        let mut values = reply.value32()?;
        Some(ScreenRect {
            x: i32::try_from(values.next()?).ok()?,
            y: i32::try_from(values.next()?).ok()?,
            width: values.next()?,
            height: values.next()?,
        })
    }

    /// Which of the modifier keys (any side) are down right now.
    pub fn modifiers_down(&self) -> Vec<u16> {
        let Some(keys) = self
            .conn
            .query_keymap()
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.keys)
        else {
            return Vec::new();
        };
        MODIFIER_VKS
            .iter()
            .copied()
            .filter(|vk| {
                x11_keycode_from_vk(*vk).is_some_and(|code| {
                    keys.get(usize::from(code / 8))
                        .is_some_and(|byte| byte & (1 << (code % 8)) != 0)
                })
            })
            .collect()
    }

    /// Asks the window manager to make `window` the active one (what a pager does).
    pub fn activate(&self, window: Window) -> PortResult<()> {
        let event = ClientMessageEvent::new(
            32,
            window,
            self.active_window,
            ClientMessageData::from([2, 0, 0, 0, 0]),
        );
        self.conn
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map_err(|error| failure(&error.to_string()))?;
        self.conn
            .flush()
            .map_err(|error| failure(&error.to_string()))
    }
}

fn failure(detail: &str) -> PortError {
    PortError::new(AppError::Internal).with_detail(format!("X11: {detail}"))
}

/// The rectangle both `a` and `b` cover, if any.
fn intersect(a: &ScreenRect, b: &ScreenRect) -> Option<ScreenRect> {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (i64::from(a.x) + i64::from(a.width)).min(i64::from(b.x) + i64::from(b.width));
    let bottom = (i64::from(a.y) + i64::from(a.height)).min(i64::from(b.y) + i64::from(b.height));
    let width = u32::try_from(right - i64::from(left))
        .ok()
        .filter(|w| *w > 0)?;
    let height = u32::try_from(bottom - i64::from(top))
        .ok()
        .filter(|h| *h > 0)?;
    Some(ScreenRect {
        x: left,
        y: top,
        width,
        height,
    })
}

/// The kernel's short name of process `pid`.
fn comm_of(pid: u32) -> Option<String> {
    let name = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_intersect_or_do_not() {
        let monitor = ScreenRect {
            x: 1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let work = ScreenRect {
            x: 0,
            y: 0,
            width: 3840,
            height: 1040,
        };
        assert_eq!(
            intersect(&monitor, &work),
            Some(ScreenRect {
                x: 1920,
                y: 0,
                width: 1920,
                height: 1040
            })
        );
        let far = ScreenRect {
            x: 0,
            y: 2000,
            width: 10,
            height: 10,
        };
        assert_eq!(intersect(&monitor, &far), None);
    }

    #[test]
    fn this_process_has_a_name() {
        assert!(comm_of(std::process::id()).is_some());
        assert_eq!(comm_of(0), None);
    }

    /// Needs an X server (DISPLAY); an X11 or XWayland session has one.
    #[test]
    fn a_live_session_answers_without_failing() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        if let Ok(session) = X11Session::connect() {
            let _ = session.active_window();
            let _ = session.modifiers_down();
        }
    }
}
