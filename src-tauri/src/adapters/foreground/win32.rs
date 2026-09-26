/*!
 * SOURCE OF TRUTH KEYWORDS: Win32ForegroundApp, last_external, GetForegroundWindow, QueryFullProcessImageNameW, MonitorFromWindow, rcWork, TokenIntegrityLevel, integrity level, UIPI, elevated target
 * WHAT:  Win32ForegroundApp: ForegroundApp on the Win32 window manager. Reports the focused top-level window, its
 *        process id and executable name, the work area of the monitor showing it, and whether its process runs at
 *        a higher integrity level than Echo; `last_external` reports the same facts for the last app the user was in
 *        outside Echo and the shell, which its ForegroundTracker (tracker.rs) keeps current.
 * WHY:   The target is captured once when a take starts (05 W3) and all three facts ride with it: the exe name
 *        becomes `transcripts.app_name`, the work area places the pill in physical pixels above the taskbar on
 *        mixed-DPI setups (05 W15; Tauri makes the process per-monitor DPI aware v2, so rcWork is physical), and
 *        the integrity comparison is how UIPI decides whether synthetic input may reach the window (05 W2).
 *        The comparison reads the mandatory-label RID of both process tokens instead of asking "is it an admin",
 *        because UIPI compares integrity levels, not group membership. A target whose token cannot be read is
 *        treated as elevated: a copy with a toast beats a paste that silently goes nowhere, since SendInput
 *        reports success even when UIPI drops it. The executable name is best effort (a protected process may
 *        refuse the query) and is None then. Only PROCESS_QUERY_LIMITED_INFORMATION is requested, the right that
 *        works across integrity levels.
 *        A tracker that cannot start is logged and `last_external` answers None (the text is then copied).
 * WHERE: Built by app/bootstrap into CommandCtx; `current` / `last_external` are called by the session actor's Arm
 *        and by paste-last under the take's TargetRule, through `dyn ForegroundApp`.
 */

use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::Path};

use windows::{
    Win32::{
        Foundation::{HANDLE, HWND},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        Security::{
            GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation,
            TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel,
        },
        System::Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    },
    core::PWSTR,
};

use super::tracker::ForegroundTracker;
use crate::{
    adapters::win32::{OwnedHandle, window_handle},
    ports::ForegroundApp,
    types::{AppTarget, PortResult, ScreenRect},
};

/// SECURITY_MANDATORY_MEDIUM_RID: the level a normal desktop app runs at, assumed for Echo if its own token is
/// unreadable (Echo is never run elevated, 05 W2).
const MEDIUM_INTEGRITY: u32 = 0x2000;

/// Longest executable path QueryFullProcessImageNameW can return (the extended-length path limit, in UTF-16 units).
const MAX_IMAGE_PATH: usize = 32_768;

/// The focused window through the Win32 window manager.
pub struct Win32ForegroundApp {
    /// Echo's own integrity level, read once: it never changes while the process runs.
    own_integrity: u32,
    /// The last app in front outside Echo and the shell; None when its hook could not start.
    tracker: Option<ForegroundTracker>,
}

impl Default for Win32ForegroundApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Win32ForegroundApp {
    pub fn new() -> Self {
        // SAFETY: GetCurrentProcess returns a pseudo-handle that is always valid and needs no closing.
        let own_integrity = integrity_level(unsafe { GetCurrentProcess() }).unwrap_or_else(|detail| {
            tracing::warn!(%detail, "Echo's own integrity level is unreadable; assuming medium");
            MEDIUM_INTEGRITY
        });
        let tracker = ForegroundTracker::start()
            .inspect_err(|error| {
                tracing::warn!(
                    detail = error.detail(),
                    "the last app in front cannot be followed; tray actions will copy instead of paste"
                );
            })
            .ok();
        Self {
            own_integrity,
            tracker,
        }
    }

    /// The AppTarget for `window`; None when it closed meanwhile.
    fn target(&self, window: HWND) -> Option<AppTarget> {
        let mut process_id = 0u32;
        // SAFETY: `process_id` outlives the call and receives the owning process id.
        let thread = unsafe { GetWindowThreadProcessId(window, Some(&raw mut process_id)) };
        if thread == 0 || process_id == 0 {
            return None;
        }
        let (exe_name, integrity) = match open_process(process_id) {
            Ok(process) => (
                exe_name(&process),
                integrity_level(process.raw())
                    .inspect_err(|detail| {
                        tracing::debug!(%detail, "target integrity level unreadable; treated as elevated");
                    })
                    .ok(),
            ),
            Err(detail) => {
                tracing::debug!(%detail, "target process cannot be queried; treated as elevated");
                (None, None)
            }
        };
        Some(AppTarget {
            window: window_handle(window),
            process_id,
            exe_name,
            work_area: work_area(window),
            elevated: integrity.is_none_or(|level| level > self.own_integrity),
        })
    }
}

impl ForegroundApp for Win32ForegroundApp {
    fn current(&self) -> PortResult<Option<AppTarget>> {
        // SAFETY: GetForegroundWindow takes no arguments and returns a null handle when no window has focus.
        let window = unsafe { GetForegroundWindow() };
        if window.is_invalid() {
            return Ok(None);
        }
        // A window that closed between the two calls means nothing has focus any more.
        Ok(self.target(window))
    }

    fn last_external(&self) -> PortResult<Option<AppTarget>> {
        Ok(self
            .tracker
            .as_ref()
            .and_then(ForegroundTracker::last)
            .and_then(|window| self.target(window)))
    }
}

fn open_process(process_id: u32) -> Result<OwnedHandle, String> {
    // SAFETY: plain value arguments; the returned handle is owned by OwnedHandle and closed on drop.
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map(OwnedHandle::new)
        .map_err(|error| format!("OpenProcess({process_id}) failed: {error}"))
}

/// The file name of the process's executable, e.g. `notepad.exe`.
fn exe_name(process: &OwnedHandle) -> Option<String> {
    let mut path = vec![0u16; MAX_IMAGE_PATH];
    let mut length = u32::try_from(path.len()).unwrap_or(u32::MAX);
    // SAFETY: `path` holds `length` UTF-16 units and outlives the call; on success `length` is the count written.
    unsafe {
        QueryFullProcessImageNameW(
            process.raw(),
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &raw mut length,
        )
    }
    .ok()?;
    let written = path.get(..usize::try_from(length).ok()?)?;
    let path = OsString::from_wide(written);
    Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/**
 * SOURCE OF TRUTH KEYWORDS: integrity_level, TokenIntegrityLevel, mandatory label RID, GetSidSubAuthority
 * WHAT:  The mandatory integrity RID of `process`'s token (0x2000 medium, 0x3000 high, 0x4000 system).
 * WHY:   UIPI blocks input into a window whose process has a higher RID (05 W2). The label is the SID's last
 *        sub-authority. The buffer is u64-backed so TOKEN_MANDATORY_LABEL, which holds a pointer, is aligned.
 * WHERE: Win32ForegroundApp::new (Echo's own level) and `current` (the target's).
 */
fn integrity_level(process: HANDLE) -> Result<u32, String> {
    let mut token = HANDLE::default();
    // SAFETY: `token` outlives the call and receives a handle that OwnedHandle closes.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) }
        .map_err(|error| format!("OpenProcessToken failed: {error}"))?;
    let token = OwnedHandle::new(token);
    let mut needed = 0u32;
    // SAFETY: a size query: no buffer and zero length; only `needed` is written. It fails by design with
    // ERROR_INSUFFICIENT_BUFFER, so its result is not an error here.
    let _ =
        unsafe { GetTokenInformation(token.raw(), TokenIntegrityLevel, None, 0, &raw mut needed) };
    let bytes = usize::try_from(needed).map_err(|error| error.to_string())?;
    if bytes < size_of::<TOKEN_MANDATORY_LABEL>() {
        return Err(format!("token integrity label size {bytes} is too small"));
    }
    let mut buffer = vec![0u64; bytes.div_ceil(size_of::<u64>())];
    // SAFETY: `buffer` holds at least `needed` bytes, is 8-byte aligned and outlives the call.
    unsafe {
        GetTokenInformation(
            token.raw(),
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &raw mut needed,
        )
    }
    .map_err(|error| format!("GetTokenInformation(TokenIntegrityLevel) failed: {error}"))?;
    // SAFETY: the call succeeded, so the buffer starts with an initialised, aligned TOKEN_MANDATORY_LABEL whose SID
    // points into the same buffer, which lives until the end of this function.
    let sid = unsafe { &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>() }
        .Label
        .Sid;
    // SAFETY: `sid` is the valid SID returned above; the count pointer points into it.
    let count = unsafe { *GetSidSubAuthorityCount(sid) };
    let last = u32::from(count)
        .checked_sub(1)
        .ok_or_else(|| String::from("integrity SID has no sub-authority"))?;
    // SAFETY: `last` is below the SID's sub-authority count, so the returned pointer is inside the SID.
    Ok(unsafe { *GetSidSubAuthority(sid, last) })
}

/// The work area (screen minus taskbar) of the monitor showing `window`, in physical pixels.
fn work_area(window: HWND) -> Option<ScreenRect> {
    // SAFETY: MONITOR_DEFAULTTONEAREST always returns a monitor for any window value.
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).ok()?,
        ..MONITORINFO::default()
    };
    // SAFETY: `info` is a MONITORINFO with cbSize set and outlives the call.
    if !unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() {
        return None;
    }
    let area = info.rcWork;
    Some(ScreenRect {
        x: area.left,
        y: area.top,
        width: u32::try_from(area.right.checked_sub(area.left)?).ok()?,
        height: u32::try_from(area.bottom.checked_sub(area.top)?).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use windows::Win32::System::Threading::GetCurrentProcessId;

    use super::*;

    /// Echo's own process is always reachable: same token, same level, and its executable name is readable.
    #[test]
    fn own_process_integrity_and_name_are_readable() {
        let app = Win32ForegroundApp::new();
        // SAFETY: the pseudo-handle is always valid.
        let own = integrity_level(unsafe { GetCurrentProcess() }).unwrap();
        assert_eq!(own, app.own_integrity);
        // SAFETY: no arguments.
        let process = open_process(unsafe { GetCurrentProcessId() }).unwrap();
        assert_eq!(integrity_level(process.raw()).unwrap(), own);
        let name = exe_name(&process).unwrap();
        assert!(
            Path::new(&name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe")),
            "{name}"
        );
    }

    #[test]
    fn a_missing_process_cannot_be_opened() {
        assert!(open_process(u32::MAX).is_err());
    }

    /// The desktop always has a monitor, so any window value maps to a non-empty work area.
    #[test]
    fn work_area_is_a_real_rectangle() {
        // SAFETY: no arguments.
        let area = work_area(unsafe { GetForegroundWindow() }).unwrap();
        assert!(area.width > 0 && area.height > 0, "{area:?}");
    }

    #[test]
    fn current_never_fails() {
        let target = Win32ForegroundApp::new().current().unwrap();
        if let Some(target) = target {
            assert_ne!(target.process_id, 0);
        }
    }
}
