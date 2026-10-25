/*!
 * SOURCE OF TRUTH KEYWORDS: WaylandVirtualKeyboardInserter, zwp_virtual_keyboard_v1, wlroots virtual keyboard, Hyprland paste, Sway paste, synthetic Ctrl+V Wayland, memfd XKB keymap, paste settle delay
 * WHAT:  WaylandVirtualKeyboardInserter: TextInserter that pastes what the clipboard holds with a synthetic Ctrl+V
 *        typed on a virtual keyboard the compositor creates for Echo (`zwp_virtual_keyboard_v1`). `available()`
 *        says whether the compositor offers the protocol. Caps: uses the clipboard, reaches any focused window.
 * WHY:   Wayland forbids one client from typing into another, except through protocols the compositor chooses to
 *        offer; the virtual keyboard is what wlroots-based compositors (Hyprland, Sway, river, labwc) offer, and it
 *        is the same tool wtype and ydotool-less setups use. It needs no privileges and no udev rule: the
 *        compositor, not the kernel, grants it, and it only exists while Echo holds the object. GNOME and KDE do not
 *        offer it; they get the RemoteDesktop portal instead. The text is already on the clipboard
 *        (`uses_clipboard`), so only Ctrl+V is typed, never the transcript itself. A virtual keyboard needs an XKB
 *        keymap before its first key; the one below names just Ctrl, Shift and V, compiled by the compositor from
 *        the system's `complete` types, and is passed in a memfd as the protocol requires. Modifiers are sent as
 *        explicit state (the virtual keyboard has its own, apart from the physical one), then the key, then
 *        cleared. The user usually still holds the dictation chord when the take ends, so the paste waits
 *        SETTLE first; physical Alt/Ctrl held longer than that would combine with the virtual keys, the Wayland
 *        counterpart of the Windows adapter's modifier wait, which cannot be done here because a client cannot read
 *        the physical keyboard. Each paste connects, types and disconnects, so a compositor restart never leaves
 *        a stale object.
 * WHERE: Built by app/platform.rs on Linux for a Wayland session whose compositor offers the protocol; called
 *        through `dyn TextInserter` by pipeline/delivery.rs.
 */

use std::{
    fs::File,
    io::Write,
    os::fd::{AsFd, FromRawFd, OwnedFd},
    thread,
    time::{Duration, Instant},
};

use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::{GlobalList, GlobalListContents, registry_queue_init},
    protocol::{wl_registry::WlRegistry, wl_seat::WlSeat},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

use crate::{
    ports::TextInserter,
    types::{AppError, AppTarget, InserterCaps, Permission, PortError, PortResult},
};

/// How long the paste waits for the user to let go of the dictation chord.
const SETTLE: Duration = Duration::from_millis(150);

/// Pause between the key-down and the key-up, so slow clients see both.
const KEY_HOLD: Duration = Duration::from_millis(12);

/// The evdev code of V, the key typed under Control.
const KEY_V: u32 = 47;

/// The XKB modifier mask of Control (the third real modifier) in the keymap below.
const MOD_CONTROL: u32 = 4;

/// wl_keyboard.key_state and keymap_format values.
const KEY_RELEASED: u32 = 0;
const KEY_PRESSED: u32 = 1;
const KEYMAP_FORMAT_XKB_V1: u32 = 1;

/// Just the keys Echo types, Control and V; keycodes are evdev + 8.
const KEYMAP: &str = "xkb_keymap {
  xkb_keycodes \"echo\" { minimum = 8; maximum = 255; <LCTL> = 37; <AB04> = 55; };
  xkb_types \"echo\" { include \"complete\" };
  xkb_compat \"echo\" { include \"complete\" };
  xkb_symbols \"echo\" {
    key <LCTL> { [ Control_L ] };
    key <AB04> { [ v, V ] };
    modifier_map Control { <LCTL> };
  };
};
";

#[derive(Debug, Default)]
pub struct WaylandVirtualKeyboardInserter;

impl WaylandVirtualKeyboardInserter {
    pub const fn new() -> Self {
        Self
    }

    /// Whether the compositor offers the virtual-keyboard protocol and a seat.
    pub fn available() -> bool {
        Connection::connect_to_env()
            .ok()
            .and_then(|connection| registry_queue_init::<State>(&connection).ok())
            .is_some_and(|(globals, _)| {
                has_global(&globals, "zwp_virtual_keyboard_manager_v1")
                    && has_global(&globals, "wl_seat")
            })
    }
}

fn has_global(globals: &GlobalList, interface: &str) -> bool {
    globals
        .contents()
        .with_list(|list| list.iter().any(|global| global.interface == interface))
}

impl TextInserter for WaylandVirtualKeyboardInserter {
    fn caps(&self) -> InserterCaps {
        InserterCaps {
            can_target_elevated: true,
            uses_clipboard: true,
        }
    }

    fn insert(&self, _target: &AppTarget, _text: &str) -> PortResult<()> {
        thread::sleep(SETTLE);
        paste().map_err(|detail| {
            PortError::new(AppError::PermissionDenied {
                permission: Permission::InputInjection,
            })
            .with_detail(format!("the virtual keyboard could not paste: {detail}"))
        })
    }
}

/// Connects, types Ctrl+V on a fresh virtual keyboard and disconnects.
fn paste() -> Result<(), String> {
    let connection = Connection::connect_to_env().map_err(|error| error.to_string())?;
    let (globals, mut queue) =
        registry_queue_init::<State>(&connection).map_err(|error| error.to_string())?;
    let qh = queue.handle();
    let seat: WlSeat = globals
        .bind(&qh, 1..=1, ())
        .map_err(|error| format!("no seat: {error}"))?;
    let manager: ZwpVirtualKeyboardManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|error| format!("the compositor has no virtual keyboard: {error}"))?;
    let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());

    let keymap = keymap_file().map_err(|error| format!("keymap: {error}"))?;
    let size = u32::try_from(KEYMAP.len() + 1).map_err(|_| "keymap too large".to_owned())?;
    keyboard.keymap(KEYMAP_FORMAT_XKB_V1, keymap.as_fd(), size);
    let mut state = State;
    queue
        .roundtrip(&mut state)
        .map_err(|error| error.to_string())?;

    let started = Instant::now();
    let now = || u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    keyboard.modifiers(MOD_CONTROL, 0, 0, 0);
    keyboard.key(now(), KEY_V, KEY_PRESSED);
    queue.flush().map_err(|error| error.to_string())?;
    thread::sleep(KEY_HOLD);
    keyboard.key(now(), KEY_V, KEY_RELEASED);
    keyboard.modifiers(0, 0, 0, 0);
    queue
        .roundtrip(&mut state)
        .map_err(|error| error.to_string())?;
    keyboard.destroy();
    queue
        .roundtrip(&mut state)
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// The keymap in an anonymous file, NUL-terminated, as the protocol wants it.
fn keymap_file() -> std::io::Result<File> {
    // SAFETY: memfd_create takes a NUL-terminated name and flags, and returns a new descriptor or -1.
    let fd = unsafe { libc::memfd_create(c"echo-keymap".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `fd` was just created and nothing else owns it.
    let mut file = File::from(unsafe { OwnedFd::from_raw_fd(fd) });
    file.write_all(KEYMAP.as_bytes())?;
    file.write_all(&[0])?;
    Ok(file)
}

struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: <WlSeat as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardManagerV1,
        _: <ZwpVirtualKeyboardManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardV1,
        _: <ZwpVirtualKeyboardV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keymap_names_the_typed_keys_and_ends_in_a_terminated_file() {
        assert!(KEYMAP.contains("<AB04> = 55"), "V is evdev 47 + 8");
        assert_eq!(KEY_V + 8, 55);
        let mut file = keymap_file().unwrap();
        use std::io::{Read, Seek, SeekFrom};
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes.len(), KEYMAP.len() + 1);
        assert_eq!(bytes.last(), Some(&0));
    }

    /// Whatever the compositor offers, asking must not fail or hang.
    #[test]
    fn asking_whether_the_protocol_exists_is_harmless() {
        let _ = WaylandVirtualKeyboardInserter::available();
    }
}
