/*!
 * SOURCE OF TRUTH KEYWORDS: PortalRemoteDesktopInserter, RemoteDesktop portal, libei, reis, EI keyboard, GNOME paste, KDE paste, restore token, persist mode, emulated input, synthetic Ctrl+V portal
 * WHAT:  PortalRemoteDesktopInserter: TextInserter that pastes what the clipboard holds with a synthetic Ctrl+V sent
 *        through the desktop's RemoteDesktop portal and libei. A worker thread owns the portal session and the EI
 *        connection; `insert` asks it to type and waits for the answer. The first paste opens the connection (the
 *        desktop asks the user once);
 * WHY:   GNOME and KDE let a client type into other windows only through this portal, after the user approved it in a
 *        dialog the desktop draws, which is the permission model Wayland wants: Echo gets no keyboard device and no
 *        udev rule, and the approval can be revoked in the desktop's settings. The approval is remembered with the
 *        portal's restore token (stored in the app data folder, readable by the user only), so later sessions and
 *        restarts ask nothing. The connection is kept open between pastes (reconnecting is slow and could prompt);
 *        it is reopened when the compositor drops it. libei sends evdev key codes, so no keymap is needed. A paste
 *        waits SETTLE first because the dictation chord is usually still held; the first paste may take as long as
 *        the user takes to answer the dialog (FIRST_USE_TIMEOUT). The text is already on the clipboard
 *        (`uses_clipboard`), so only Ctrl+V is typed.
 * WHERE: Built by app/platform.rs on Linux for a Wayland session without the wlroots virtual keyboard; called through
 *        `dyn TextInserter` by pipeline/delivery.rs.
 */

use std::{
    fs,
    os::unix::{fs::OpenOptionsExt, net::UnixStream},
    path::PathBuf,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

use ashpd::{
    desktop::{
        CreateSessionOptions, PersistMode, Session,
        remote_desktop::{
            ConnectToEISOptions, DeviceType, RemoteDesktop, SelectDevicesOptions, StartOptions,
        },
    },
    enumflags2::BitFlags,
};
use futures_util::StreamExt;
use reis::{
    ei::{self, keyboard::KeyState},
    event::{DeviceCapability, EiEvent, EiEventConverter},
    tokio::EiEventStream,
};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

use crate::{
    adapters::linux::register_app,
    ports::TextInserter,
    types::{AppError, AppTarget, InserterCaps, Permission, PortError, PortResult},
};

/// How long the paste waits for the user to let go of the dictation chord.
const SETTLE: Duration = Duration::from_millis(150);

/// The first paste includes the desktop's approval dialog; later ones are quick.
const FIRST_USE_TIMEOUT: Duration = Duration::from_secs(90);
const PASTE_TIMEOUT: Duration = Duration::from_secs(8);

/// How long a key stays down, so slow clients see both edges.
const KEY_HOLD: Duration = Duration::from_millis(12);

/// evdev codes: Left Ctrl, V.
const KEY_LEFTCTRL: u32 = 29;
const KEY_V: u32 = 47;

struct Paste {
    reply: mpsc::Sender<Result<(), String>>,
}

pub struct PortalRemoteDesktopInserter {
    requests: tokio_mpsc::UnboundedSender<Paste>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    /// Set once a paste has succeeded, after which the shorter timeout applies.
    approved: std::sync::atomic::AtomicBool,
}

impl PortalRemoteDesktopInserter {
    /// Starts the worker; nothing contacts the portal until the first paste.
    pub fn new(app_id: &str, token_file: PathBuf) -> PortResult<Self> {
        let (requests, received) = tokio_mpsc::unbounded_channel();
        let (stop, stopped) = oneshot::channel();
        let app_id = app_id.to_owned();
        let thread = thread::Builder::new()
            .name("echo-portal-paste".to_owned())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    tracing::error!("the paste runtime could not start");
                    return;
                };
                runtime.block_on(serve(app_id, token_file, received, stopped));
            })
            .map_err(|error| failure(&error.to_string()))?;
        Ok(Self {
            requests,
            stop: Some(stop),
            thread: Some(thread),
            approved: std::sync::atomic::AtomicBool::new(false),
        })
    }
}

impl TextInserter for PortalRemoteDesktopInserter {
    fn caps(&self) -> InserterCaps {
        InserterCaps {
            can_target_elevated: true,
            uses_clipboard: true,
        }
    }

    fn insert(&self, _target: &AppTarget, _text: &str) -> PortResult<()> {
        thread::sleep(SETTLE);
        let (reply, answered) = mpsc::channel();
        self.requests
            .send(Paste { reply })
            .map_err(|_| failure("the paste thread has stopped"))?;
        let timeout = if self.approved.load(std::sync::atomic::Ordering::Acquire) {
            PASTE_TIMEOUT
        } else {
            FIRST_USE_TIMEOUT
        };
        match answered.recv_timeout(timeout) {
            Ok(Ok(())) => {
                self.approved
                    .store(true, std::sync::atomic::Ordering::Release);
                Ok(())
            }
            Ok(Err(detail)) => Err(failure(&detail)),
            Err(_) => Err(failure("the desktop did not answer in time")),
        }
    }
}

impl Drop for PortalRemoteDesktopInserter {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("the paste thread panicked");
        }
    }
}

fn failure(detail: &str) -> PortError {
    PortError::new(AppError::PermissionDenied {
        permission: Permission::InputInjection,
    })
    .with_detail(format!(
        "the desktop's keyboard portal could not paste: {detail}"
    ))
}

/// A live portal session and its EI keyboard.
struct Connection {
    // Kept so the session lives as long as the connection.
    _session: Session<RemoteDesktop>,
    events: EiEventStream,
    converter: EiEventConverter,
    keyboard: Option<(reis::event::Device, ei::Keyboard)>,
    sequence: u32,
}

/// The worker body: serves paste requests, opening or reopening the connection as needed.
async fn serve(
    app_id: String,
    token_file: PathBuf,
    mut requests: tokio_mpsc::UnboundedReceiver<Paste>,
    stopped: oneshot::Receiver<()>,
) {
    register_app(&app_id).await;
    let mut connection: Option<Connection> = None;
    tokio::pin!(stopped);
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            Some(paste) = requests.recv() => {
                let result = paste_once(&mut connection, &token_file).await;
                if result.is_err() {
                    // A broken connection is rebuilt by the next paste.
                    connection = None;
                }
                drop(paste.reply.send(result));
            }
            else => break,
        }
    }
}

async fn paste_once(
    connection: &mut Option<Connection>,
    token_file: &PathBuf,
) -> Result<(), String> {
    if connection.is_none() {
        *connection = Some(connect(token_file).await?);
    }
    let Some(live) = connection.as_mut() else {
        return Err("no connection".to_owned());
    };
    live.settle().await?;
    live.type_paste().await
}

/// Opens a RemoteDesktop session for the keyboard and the EI connection it hands out.
async fn connect(token_file: &PathBuf) -> Result<Connection, String> {
    let portal = RemoteDesktop::new().await.map_err(|e| e.to_string())?;
    let session = portal
        .create_session(CreateSessionOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    let stored = fs::read_to_string(token_file).ok();
    portal
        .select_devices(
            &session,
            SelectDevicesOptions::default()
                .set_devices(BitFlags::from(DeviceType::Keyboard))
                .set_persist_mode(PersistMode::ExplicitlyRevoked)
                .set_restore_token(stored.as_deref().map(str::trim)),
        )
        .await
        .map_err(|e| e.to_string())?;
    let started = portal
        .start(&session, None, StartOptions::default())
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| format!("the desktop refused keyboard control: {e}"))?;
    if let Some(token) = started.restore_token() {
        save_token(token_file, token);
    }
    let fd = portal
        .connect_to_eis(&session, ConnectToEISOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    let context = ei::Context::new(UnixStream::from(fd)).map_err(|e| e.to_string())?;
    let mut events = EiEventStream::new(context.clone()).map_err(|e| e.to_string())?;
    let handshake =
        reis::tokio::ei_handshake(&mut events, "Echo", ei::handshake::ContextType::Sender)
            .await
            .map_err(|e| e.to_string())?;
    let mut connection = Connection {
        _session: session,
        events,
        converter: EiEventConverter::new(&context, handshake),
        keyboard: None,
        sequence: 0,
    };
    // Seats and devices arrive over a few round trips; wait for the keyboard to be usable.
    tokio::time::timeout(Duration::from_secs(5), async {
        while connection.keyboard.is_none() {
            connection.next_event().await?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|_| "the desktop offered no keyboard".to_owned())??;
    Ok(connection)
}

/// Writes the portal's restore token readable by the user only; failure just means asking again next time.
fn save_token(file: &PathBuf, token: &str) {
    use std::io::Write;
    let written = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(file)
        .and_then(|mut f| f.write_all(token.as_bytes()));
    if let Err(error) = written {
        tracing::debug!(%error, "the paste approval could not be remembered");
    }
}

impl Connection {
    /// Reads one event from the compositor and updates seats and devices.
    async fn next_event(&mut self) -> Result<(), String> {
        let Some(item) = self.events.next().await else {
            return Err("the desktop closed the keyboard connection".to_owned());
        };
        let result = item.map_err(|e| e.to_string())?;
        let reis::PendingRequestResult::Request(request) = result else {
            return Ok(());
        };
        self.converter
            .handle_event(request)
            .map_err(|e| e.to_string())?;
        while let Some(event) = self.converter.next_event() {
            match event {
                EiEvent::SeatAdded(added) => added
                    .seat
                    .bind_capabilities(DeviceCapability::Keyboard.into()),
                EiEvent::DeviceResumed(resumed) => {
                    if let Some(keyboard) = resumed.device.interface::<ei::Keyboard>() {
                        self.keyboard = Some((resumed.device, keyboard));
                    }
                }
                EiEvent::DevicePaused(paused) => {
                    if self
                        .keyboard
                        .as_ref()
                        .is_some_and(|(device, _)| *device == paused.device)
                    {
                        self.keyboard = None;
                    }
                }
                EiEvent::DeviceRemoved(removed) => {
                    if self
                        .keyboard
                        .as_ref()
                        .is_some_and(|(device, _)| *device == removed.device)
                    {
                        self.keyboard = None;
                    }
                }
                EiEvent::Disconnected(_) => {
                    return Err("the desktop ended the keyboard connection".to_owned());
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Applies whatever the compositor has already sent (a paused keyboard, a disconnect) without waiting for more.
    async fn settle(&mut self) -> Result<(), String> {
        while tokio::time::timeout(Duration::ZERO, self.next_event())
            .await
            .is_ok()
        {}
        if self.keyboard.is_none() {
            return Err("the keyboard is not available".to_owned());
        }
        Ok(())
    }

    /// Types Ctrl+V.
    async fn type_paste(&mut self) -> Result<(), String> {
        let Some((device, keyboard)) = self.keyboard.clone() else {
            return Err("the keyboard is not available".to_owned());
        };
        let serial = self.converter.connection().serial();
        let ei_device = device.device();
        self.sequence = self.sequence.wrapping_add(1);
        ei_device.start_emulating(serial, self.sequence);
        keyboard.key(KEY_LEFTCTRL, KeyState::Press);
        keyboard.key(KEY_V, KeyState::Press);
        ei_device.frame(serial, micros());
        self.flush()?;
        tokio::time::sleep(KEY_HOLD).await;
        keyboard.key(KEY_V, KeyState::Released);
        keyboard.key(KEY_LEFTCTRL, KeyState::Released);
        ei_device.frame(serial, micros());
        ei_device.stop_emulating(serial);
        self.flush()
    }

    fn flush(&self) -> Result<(), String> {
        self.converter
            .connection()
            .flush()
            .map_err(|e| e.to_string())
    }
}

/// Monotonic microseconds, the clock libei frames carry.
fn micros() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes into the live timespec above.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    u64::try_from(now.tv_sec).unwrap_or(0) * 1_000_000
        + u64::try_from(now.tv_nsec).unwrap_or(0) / 1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_file_is_private_and_overwritten() {
        let dir = std::env::temp_dir().join(format!("echo-token-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("remote-desktop.token");
        save_token(&file, "first");
        save_token(&file, "second");
        assert_eq!(fs::read_to_string(&file).unwrap(), "second");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(fs::remove_dir_all(&dir));
    }

    #[test]
    fn a_timestamp_moves_forward() {
        let first = micros();
        thread::sleep(Duration::from_millis(2));
        assert!(micros() > first);
    }
}
