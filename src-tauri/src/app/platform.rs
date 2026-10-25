/*!
 * SOURCE OF TRUTH KEYWORDS: platform adapters, per-OS adapter choice, composition root helpers, cfg(windows), cfg(target_os = "linux"), native adapters
 * WHAT:  The operating-system adapters bootstrap needs, one function each: the foreground app, system appearance,
 *        privacy consent, worker scheduler, GPU list, clipboard, text inserter, hotkeys, launcher, process stats, sound
 *        player, microphone capture, the power watch and the start-at-sign-in entry. Each returns the Windows adapter
 *        on Windows and the Linux one on Linux.
 * WHY:   The composition root is the only place that names a concrete adapter (02 §3.2); keeping the `cfg` choices
 *        here leaves bootstrap.rs one readable sequence for both systems. The Linux hotkey, paste and foreground
 *        adapters depend on the display server (X11 or Wayland), which `DisplayServer::detect` reads once; a
 *        session no adapter fits gets the "unavailable" adapter whose caps say so, never a failed startup.
 * WHERE: Called by app/bootstrap.rs `start`.
 */

use std::sync::Arc;

#[cfg(target_os = "linux")]
use crate::adapters::{
    audio::CpalCapture,
    consent::LinuxPrivacyConsent,
    foreground::{UnknownFocusForeground, X11ForegroundApp},
    gpu::NoGraphicsAdapters,
    hotkey::{ChordHotkeys, PortalGlobalShortcuts, X11XInput},
    inserter::{
        ClipboardOnlyInserter, PortalRemoteDesktopInserter, WaylandVirtualKeyboardInserter,
        X11XTestInserter,
    },
    launcher::XdgLauncher,
    linux::{DisplayServer, x11::X11Session},
    process::LinuxProcessStats,
    scheduler::LinuxWorkerScheduler,
    sound::CpalSoundPlayer,
};
#[cfg(windows)]
use crate::adapters::{
    audio::CpalCapture, consent::Win32PrivacyConsent, foreground::Win32ForegroundApp,
    gpu::DxgiGraphicsAdapters, hotkey::LowLevelKeyboardHotkeys, inserter::Win32SendInputInserter,
    launcher::Win32ShellLauncher, process::Win32ProcessStats, scheduler::Win32WorkerScheduler,
    sound::Win32SoundPlayer,
};
use crate::{
    adapters::{appearance, clipboard::ArboardClipboard},
    ports::{
        AudioCapture, Clipboard, ForegroundApp, GraphicsAdapters, HotkeyService, PrivacyConsent,
        ProcessStats, SoundPlayer, SystemAppearance, SystemLauncher, TextInserter, WorkerScheduler,
    },
    registry::hotkeys,
    types::{AppPaths, PortResult},
};

pub fn foreground() -> Arc<dyn ForegroundApp> {
    #[cfg(windows)]
    return Arc::new(Win32ForegroundApp::new());
    #[cfg(target_os = "linux")]
    return match (DisplayServer::detect(), X11Session::connect()) {
        (DisplayServer::X11, Ok(session)) => Arc::new(X11ForegroundApp::new(Arc::new(session))),
        (DisplayServer::X11, Err(error)) => {
            tracing::warn!(
                detail = error.detail(),
                "the focused window cannot be read; takes are copied"
            );
            Arc::new(UnknownFocusForeground::new())
        }
        // Wayland hides other apps' windows from clients; the paste goes to whatever has focus.
        (DisplayServer::Wayland, _) => Arc::new(UnknownFocusForeground::new()),
    };
}

pub fn appearance() -> Arc<dyn SystemAppearance> {
    #[cfg(windows)]
    return Arc::new(appearance::Win32SystemAppearance::new());
    #[cfg(target_os = "linux")]
    return Arc::new(appearance::LinuxSystemAppearance::new());
}

pub fn consent() -> Arc<dyn PrivacyConsent> {
    #[cfg(windows)]
    return Arc::new(Win32PrivacyConsent::new());
    #[cfg(target_os = "linux")]
    return Arc::new(LinuxPrivacyConsent::new());
}

pub fn scheduler() -> Arc<dyn WorkerScheduler> {
    #[cfg(windows)]
    return Arc::new(Win32WorkerScheduler::new());
    #[cfg(target_os = "linux")]
    return Arc::new(LinuxWorkerScheduler::new());
}

pub fn graphics() -> Arc<dyn GraphicsAdapters> {
    #[cfg(windows)]
    return Arc::new(DxgiGraphicsAdapters::new());
    #[cfg(target_os = "linux")]
    return Arc::new(NoGraphicsAdapters::new());
}

pub fn clipboard() -> Arc<dyn Clipboard> {
    Arc::new(ArboardClipboard::new())
}

/// `app_id` and `paths` are for the Wayland portal, which remembers the user's approval under Echo's identity.
pub fn inserter(app_id: &str, paths: &AppPaths) -> Arc<dyn TextInserter> {
    #[cfg(windows)]
    {
        let _ = (app_id, paths);
        return Arc::new(Win32SendInputInserter::new());
    }
    #[cfg(target_os = "linux")]
    return match DisplayServer::detect() {
        // The protocol wlroots compositors (Hyprland, Sway) offer; no approval dialog, no privileges.
        DisplayServer::Wayland if WaylandVirtualKeyboardInserter::available() => {
            Arc::new(WaylandVirtualKeyboardInserter::new())
        }
        // GNOME and KDE: the RemoteDesktop portal, approved once by the user. A desktop without it fails the paste,
        // which Delivery turns into a copy and a toast.
        DisplayServer::Wayland => {
            match PortalRemoteDesktopInserter::new(app_id, paths.remote_desktop_token()) {
                Ok(inserter) => Arc::new(inserter),
                Err(error) => {
                    tracing::warn!(
                        detail = error.detail(),
                        "pasting through the desktop portal is off"
                    );
                    Arc::new(ClipboardOnlyInserter::new())
                }
            }
        }
        DisplayServer::X11 => match X11Session::connect() {
            Ok(session) => Arc::new(X11XTestInserter::new(Arc::new(session))),
            Err(error) => {
                tracing::warn!(detail = error.detail(), "pasting through X11 is off");
                Arc::new(ClipboardOnlyInserter::new())
            }
        },
    };
}

/// `app_id` is Echo's identifier: the Wayland shortcut portal names Echo's shortcuts after it.
pub fn hotkeys(app_id: &str) -> Arc<dyn HotkeyService> {
    #[cfg(windows)]
    {
        // The low-level hook needs no identity.
        let _ = app_id;
        return Arc::new(LowLevelKeyboardHotkeys::new());
    }
    #[cfg(target_os = "linux")]
    return match DisplayServer::detect() {
        DisplayServer::X11 => Arc::new(ChordHotkeys::new(X11XInput)),
        // Wayland gives a client no global key capture: the desktop's shortcut portal delivers the keys the user approved.
        DisplayServer::Wayland => Arc::new(PortalGlobalShortcuts::new(
            app_id,
            hotkeys::HOTKEYS
                .iter()
                .map(|spec| (spec.id.clone(), spec.label.to_string()))
                .collect(),
            &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        )),
    };
}

pub fn launcher() -> Arc<dyn SystemLauncher> {
    #[cfg(windows)]
    return Arc::new(Win32ShellLauncher::new());
    #[cfg(target_os = "linux")]
    return Arc::new(XdgLauncher::new());
}

pub fn process_stats() -> Arc<dyn ProcessStats> {
    #[cfg(windows)]
    return Arc::new(Win32ProcessStats::new());
    #[cfg(target_os = "linux")]
    return Arc::new(LinuxProcessStats::new());
}

pub fn sound_player() -> PortResult<Arc<dyn SoundPlayer>> {
    #[cfg(windows)]
    return Ok(Arc::new(Win32SoundPlayer::new()?));
    #[cfg(target_os = "linux")]
    return Ok(Arc::new(CpalSoundPlayer::new()?));
}

pub fn capture(consent: Arc<dyn PrivacyConsent>) -> Arc<dyn AudioCapture> {
    Arc::new(CpalCapture::new(consent))
}
