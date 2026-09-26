/*!
 * SOURCE OF TRUTH KEYWORDS: Win32SystemAppearance, EnableTransparency, Themes Personalize, RegNotifyChangeKeyValue, CurrentBuildNumber, Mica support, transparency watcher
 * WHAT:  Win32SystemAppearance: reads Windows "Transparency effects" (`EnableTransparency` under the user's
 *        `Themes\Personalize` key), watches that key for changes on one background thread, and declares Mica
 *        support from the OS build number (Windows 11 = build 22000 or later).
 * WHY:   The webview cannot see either fact (05 W17, 04 §2). The watch uses RegNotifyChangeKeyValue on the key
 *        itself instead of a WM_SETTINGCHANGE window hook: it needs no message window or subclassing of Tauri's
 *        windows, fires for exactly the value that matters, and a stop event ends the thread cleanly when the
 *        adapter drops. A missing value means Windows' default (transparency on). The build number comes from the
 *        registry because it is not version-shimmed; an unreadable build counts as "no Mica", which only makes
 *        the page paint its own background. Handles are wrapped in owners that close them on every path (the
 *        watched key is the shared RegistryKey).
 * WHERE: Built by app/bootstrap into CommandCtx; `listen` is called once by app/bootstrap with the
 *        pipeline's AppearanceRelay; `caps` decides whether app/windows.rs applies Mica.
 */

use std::{
    sync::{Arc, mpsc},
    thread::JoinHandle,
};

use parking_lot::Mutex;
use windows::{
    Win32::{
        Foundation::{
            ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, HANDLE, WAIT_OBJECT_0,
            WIN32_ERROR,
        },
        System::{
            Registry::{
                HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_NOTIFY, KEY_QUERY_VALUE,
                REG_NOTIFY_CHANGE_LAST_SET, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
                RegNotifyChangeKeyValue,
            },
            Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects},
        },
    },
    core::{HSTRING, PCWSTR},
};

use crate::{
    adapters::win32::{OwnedHandle, RegistryKey},
    ports::{EventSink, SystemAppearance},
    types::{AppError, AppearanceCaps, PortError, PortResult, Transparency},
};

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const ENABLE_TRANSPARENCY: &str = "EnableTransparency";
const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const CURRENT_BUILD: &str = "CurrentBuildNumber";
/// First Windows 11 build; Mica exists from here on.
const FIRST_MICA_BUILD: u32 = 22_000;
/// Build numbers are five digits today; room for a few more plus the terminator.
const BUILD_CAPACITY: usize = 16;
const WATCH_THREAD: &str = "echo-appearance";

type SinkSlot = Arc<Mutex<Option<Arc<dyn EventSink<Transparency>>>>>;

/// Windows appearance preferences through the registry.
pub struct Win32SystemAppearance {
    caps: AppearanceCaps,
    sink: SinkSlot,
    watcher: Mutex<Option<Watcher>>,
}

impl Win32SystemAppearance {
    pub fn new() -> Self {
        Self {
            caps: AppearanceCaps {
                mica: supports_mica(read_build().ok()),
            },
            sink: Arc::default(),
            watcher: Mutex::new(None),
        }
    }
}

impl Default for Win32SystemAppearance {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemAppearance for Win32SystemAppearance {
    fn caps(&self) -> AppearanceCaps {
        self.caps
    }

    fn transparency(&self) -> PortResult<Transparency> {
        read_transparency()
    }

    fn listen(&self, sink: Arc<dyn EventSink<Transparency>>) -> PortResult<()> {
        *self.sink.lock() = Some(sink);
        let mut watcher = self.watcher.lock();
        if watcher.is_none() {
            *watcher = Some(Watcher::start(Arc::clone(&self.sink))?);
        }
        Ok(())
    }
}

impl Drop for Win32SystemAppearance {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.get_mut().take() {
            watcher.stop();
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: transparency watcher thread, registry change notification, stop event, join on drop
 * WHAT:  The background thread that waits for a change under the Personalize key (or the stop event), re-reads
 *        the switch and emits it to the current sink when it differs from the last value sent.
 * WHY:   RegNotifyChangeKeyValue is one-shot, so it is re-armed before every wait; the stop event lets Drop end
 *        the wait immediately instead of leaving a thread blocked forever. The key is opened on the thread and
 *        the result reported back, so `listen` fails loudly instead of silently never firing.
 * WHERE: Started by Win32SystemAppearance::listen; stopped by its Drop.
 */
struct Watcher {
    stop: Arc<OwnedEvent>,
    thread: JoinHandle<()>,
}

impl Watcher {
    fn start(sink: SinkSlot) -> PortResult<Self> {
        let stop = Arc::new(OwnedEvent::new()?);
        let thread_stop = Arc::clone(&stop);
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name(WATCH_THREAD.into())
            .spawn(move || watch(&thread_stop, &sink, &ready_tx))
            .map_err(|error| internal(format!("appearance watcher could not start: {error}")))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self { stop, thread }),
            Ok(Err(error)) => {
                // The thread has already returned after reporting the failure.
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err(internal(
                "appearance watcher ended before it was ready".into(),
            )),
        }
    }

    fn stop(self) {
        if let Err(error) = self.stop.signal() {
            tracing::warn!(%error, "appearance watcher stop signal failed");
            return;
        }
        if self.thread.join().is_err() {
            tracing::warn!("appearance watcher thread panicked");
        }
    }
}

/// The watcher loop; reports whether it started on `ready`, then runs until `stop` is signalled.
fn watch(stop: &OwnedEvent, sink: &SinkSlot, ready: &mpsc::Sender<PortResult<()>>) {
    let setup = RegistryKey::open(HKEY_CURRENT_USER, PERSONALIZE, KEY_NOTIFY | KEY_QUERY_VALUE)
        .and_then(|key| key.ok_or_else(|| internal(format!("{PERSONALIZE} does not exist"))))
        .and_then(|key| {
            let change = OwnedEvent::new()?;
            Ok((key, change))
        });
    let (key, change) = match setup {
        Ok(parts) => parts,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    let mut last = read_transparency().ok();
    loop {
        // SAFETY: `key` and `change` are open handles owned by this thread for the whole loop.
        let armed = unsafe {
            RegNotifyChangeKeyValue(
                key.raw(),
                false,
                REG_NOTIFY_CHANGE_LAST_SET,
                Some(change.raw()),
                true,
            )
        };
        if armed != ERROR_SUCCESS {
            tracing::warn!(
                code = armed.0,
                "appearance watcher could not re-arm; transparency changes apply after restart"
            );
            return;
        }
        // SAFETY: both handles stay open until this function returns.
        let woke = unsafe { WaitForMultipleObjects(&[stop.raw(), change.raw()], false, INFINITE) };
        if woke == WAIT_OBJECT_0 {
            return;
        }
        if woke.0 != WAIT_OBJECT_0.0 + 1 {
            tracing::warn!(
                code = woke.0,
                "appearance watcher wait failed; transparency changes apply after restart"
            );
            return;
        }
        match read_transparency() {
            Ok(now) if last != Some(now) => {
                last = Some(now);
                let current = sink.lock().clone();
                if let Some(current) = current {
                    current.emit(now);
                }
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(detail = error.detail(), "transparency re-read failed");
            }
        }
    }
}

/// An owned Win32 event handle, closed on drop.
struct OwnedEvent(OwnedHandle);

impl OwnedEvent {
    /// An auto-reset event, initially not signalled.
    fn new() -> PortResult<Self> {
        // SAFETY: no security attributes and no name; the returned handle is owned by Self.
        unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
            .map(|event| Self(OwnedHandle::new(event)))
            .map_err(|error| internal(format!("CreateEventW failed: {error}")))
    }

    fn raw(&self) -> HANDLE {
        self.0.raw()
    }

    fn signal(&self) -> windows::core::Result<()> {
        // SAFETY: the handle is open for the lifetime of self.
        unsafe { SetEvent(self.0.raw()) }
    }
}

/// The transparency switch right now; a missing value is Windows' default (on).
fn read_transparency() -> PortResult<Transparency> {
    let mut value = 0u32;
    let mut bytes = u32::try_from(size_of::<u32>()).unwrap_or(u32::MAX);
    // SAFETY: `value` is a u32 and `bytes` holds its exact size, so RegGetValueW writes at most 4 bytes into it.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(PERSONALIZE),
            &HSTRING::from(ENABLE_TRANSPARENCY),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&raw mut bytes),
        )
    };
    match status {
        ERROR_SUCCESS => Ok(transparency_from(Some(value))),
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(transparency_from(None)),
        other => Err(win32_failure("read", ENABLE_TRANSPARENCY, other)),
    }
}

/// The OS build number (`CurrentBuildNumber`, a REG_SZ).
fn read_build() -> PortResult<u32> {
    let mut buffer = [0u16; BUILD_CAPACITY];
    let mut bytes = u32::try_from(size_of_val(&buffer)).unwrap_or(u32::MAX);
    // SAFETY: `buffer` outlives the call and `bytes` holds its exact size in bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(CURRENT_VERSION),
            &HSTRING::from(CURRENT_BUILD),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(win32_failure("read", CURRENT_BUILD, status));
    }
    let length = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..length]);
    text.trim()
        .parse()
        .map_err(|_| internal(format!("{CURRENT_BUILD} is not a number: {text}")))
}

/// `EnableTransparency` = 0 turns transparency off; any other value, or none, leaves it on.
fn transparency_from(value: Option<u32>) -> Transparency {
    match value {
        Some(0) => Transparency::Reduced,
        _ => Transparency::Full,
    }
}

/// Mica needs Windows 11; an unknown build gets no Mica.
fn supports_mica(build: Option<u32>) -> bool {
    build.is_some_and(|build| build >= FIRST_MICA_BUILD)
}

fn win32_failure(action: &str, name: &str, status: WIN32_ERROR) -> PortError {
    internal(format!(
        "appearance registry {action} of {name} failed: Win32 error {}",
        status.0
    ))
}

fn internal(detail: String) -> PortError {
    PortError::new(AppError::Internal).with_detail(detail)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::ports::fakes::RecordingSink;

    #[test]
    fn a_zero_switch_reduces_and_anything_else_keeps_transparency() {
        assert_eq!(transparency_from(Some(0)), Transparency::Reduced);
        assert_eq!(transparency_from(Some(1)), Transparency::Full);
        assert_eq!(transparency_from(None), Transparency::Full);
    }

    #[test]
    fn mica_starts_at_the_first_windows_11_build() {
        assert!(!supports_mica(None));
        assert!(!supports_mica(Some(19_045)));
        assert!(supports_mica(Some(22_000)));
        assert!(supports_mica(Some(26_200)));
    }

    /// Reads the real registry of the machine running the tests: whatever it says, both reads answer.
    #[test]
    fn reads_this_machines_appearance() {
        assert!(read_transparency().is_ok());
        assert!(read_build().is_ok_and(|build| build > 0));
        let appearance = Win32SystemAppearance::new();
        assert_eq!(appearance.caps().mica, supports_mica(read_build().ok()));
    }

    #[test]
    fn the_watcher_starts_replaces_its_sink_and_stops_on_drop() {
        let appearance = Win32SystemAppearance::new();
        let first = Arc::new(RecordingSink::default());
        let second = Arc::new(RecordingSink::default());
        appearance.listen(first).unwrap();
        appearance.listen(second).unwrap();
        assert!(appearance.watcher.lock().is_some());
        let started = Instant::now();
        drop(appearance);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "drop must stop the watcher promptly"
        );
    }

    #[test]
    fn a_missing_key_is_reported_as_absent() {
        let missing = RegistryKey::open(
            HKEY_CURRENT_USER,
            r"Software\Echo\NoSuchAppearanceKey",
            KEY_NOTIFY | KEY_QUERY_VALUE,
        )
        .unwrap();
        assert!(missing.is_none());
    }
}
