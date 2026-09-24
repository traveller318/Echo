/*!
 * SOURCE OF TRUTH KEYWORDS: window setup, Mica backdrop, native theme, close to hide, CloseRequested, prevent_close, on_window_event, main window, pill window attach, PILL_WINDOW
 * WHAT:  Native behaviour of the config windows: puts the Mica material behind the main window when the OS has
 *        it, applies the `general.theme` choice to every window now and whenever AppearanceChanged fires, hands
 *        the pill window to the overlay adapter, and turns a close of the main window (titlebar close, Alt+F4,
 *        taskbar "Close window") into a hide.
 * WHY:   CSS cannot blur the desktop, so Mica comes from Rust (04 §2); the page then stops painting --color-bg
 *        (`data-backdrop="mica"`). Mica is applied only when SystemAppearance caps say the OS supports it, so
 *        Windows 10 never gets a half-applied effect (Tauri swallows that error). The native theme is applied too
 *        because it tints Mica and sets WebView2's `prefers-color-scheme`, keeping the backdrop, the page and the
 *        CSS media query in agreement with `data-theme`. It listens to the same typed event the UI receives, so
 *        there is one source of appearance changes. Runs on RunEvent::Ready, after Tauri created the windows;
 *        every failure is logged, never fatal, because a missing effect only costs looks.
 *        Closing the main window hides it (04 §5, 02 §9): Echo keeps running for the global hotkey, and the
 *        window is shown again from the tray (step 25). The rule lives here, on the native CloseRequested event,
 *        so every way of closing behaves the same and the UI only asks to close.
 *        The pill window is created by Tauri (tauri.conf.json: transparent, undecorated, always on top, hidden,
 *        not focusable) but from here on only the overlay adapter touches its visibility and styles (05 W3, W16): it
 *        is attached as soon as the window exists, and Tauri's show/hide are never called on it.
 * WHERE: `setup` is called once by app::run on RunEvent::Ready and `on_window_event` is the builder's window
 *        event handler; reads the managed CommandCtx, the managed Win32OverlayWindow and pipeline/appearance.
 */

use tauri::{
    AppHandle, Manager, Runtime, Theme, Window, WindowEvent,
    window::{Effect, EffectsBuilder},
};
use tauri_specta::Event;

use std::sync::Arc;

use crate::{
    adapters::{win32::window_handle, window::Win32OverlayWindow},
    ipc::CommandCtx,
    pipeline::appearance,
    types::{AppearanceChanged, ThemePreference},
};

/// Label of the main window in tauri.conf.json.
pub const MAIN_WINDOW: &str = "main";

/// Label of the pill window in tauri.conf.json.
pub const PILL_WINDOW: &str = "pill";

/// Applies the backdrop and theme, then follows every later appearance change.
pub fn setup<R: Runtime>(app: &AppHandle<R>) {
    let Some(ctx) = app.try_state::<CommandCtx>() else {
        tracing::error!("window setup ran before the command context was managed");
        return;
    };
    let view = appearance::current(&ctx.settings(), ctx.appearance());
    if ctx.appearance().caps().mica {
        apply_mica(app);
    }
    apply_theme(app, view.theme);
    attach_pill(app);

    let handle = app.clone();
    AppearanceChanged::listen_any(app, move |event| {
        apply_theme(&handle, event.payload.0.theme);
    });
}

/**
 * SOURCE OF TRUTH KEYWORDS: on_window_event, close requested, hide instead of quit, prevent_close
 * WHAT:  Window event handler: a close request on a window that `hides_on_close` is cancelled and the window hidden.
 * WHY:   Quitting would drop the global hotkey and any take in progress; hiding keeps Echo resident (02 §9). If
 *        hiding fails the close is still cancelled, so a Windows error can never quit the app by accident.
 * WHERE: Registered by app::run through `tauri::Builder::on_window_event`.
 */
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && hides_on_close(window.label())
    {
        api.prevent_close();
        if let Err(error) = window.hide() {
            tracing::warn!(%error, window = %window.label(), "window could not be hidden");
        }
    }
}

/// Windows that hide instead of closing; the pill is never closed by the user.
fn hides_on_close(label: &str) -> bool {
    label == MAIN_WINDOW
}

/// Hands the pill window to the overlay adapter; without it the pill never shows, and dictation still works.
fn attach_pill<R: Runtime>(app: &AppHandle<R>) {
    let Some(overlay) = app.try_state::<Arc<Win32OverlayWindow>>() else {
        tracing::error!("the pill overlay was not managed before the windows were set up");
        return;
    };
    let Some(pill) = app.get_webview_window(PILL_WINDOW) else {
        tracing::warn!("no pill window to attach");
        return;
    };
    let attached = pill
        .hwnd()
        .map_err(|error| error.to_string())
        .and_then(|hwnd| {
            overlay
                .attach(window_handle(hwnd))
                .map_err(|error| error.detail().unwrap_or("no detail").to_owned())
        });
    if let Err(detail) = attached {
        tracing::warn!(%detail, "the pill window could not be attached; takes run without it");
    }
}

fn apply_mica<R: Runtime>(app: &AppHandle<R>) {
    let Some(main) = app.get_webview_window(MAIN_WINDOW) else {
        tracing::warn!("no main window to put Mica behind");
        return;
    };
    if let Err(error) = main.set_effects(EffectsBuilder::new().effect(Effect::Mica).build()) {
        tracing::warn!(%error, "Mica backdrop could not be applied");
    }
}

/// Sets the native theme of every window; `System` hands the choice back to Windows.
fn apply_theme<R: Runtime>(app: &AppHandle<R>, theme: ThemePreference) {
    let native = native_theme(theme);
    for (label, window) in app.webview_windows() {
        if let Err(error) = window.set_theme(native) {
            tracing::warn!(%error, window = %label, "window theme could not be applied");
        }
    }
}

/// The Tauri theme for a preference; `None` follows Windows.
fn native_theme(theme: ThemePreference) -> Option<Theme> {
    match theme {
        ThemePreference::System => None,
        ThemePreference::Light => Some(Theme::Light),
        ThemePreference::Dark => Some(Theme::Dark),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_main_window_hides_on_close() {
        assert!(hides_on_close(MAIN_WINDOW));
        assert!(!hides_on_close(PILL_WINDOW));
    }

    #[test]
    fn system_follows_windows_and_explicit_choices_pin_the_theme() {
        assert_eq!(native_theme(ThemePreference::System), None);
        assert_eq!(native_theme(ThemePreference::Light), Some(Theme::Light));
        assert_eq!(native_theme(ThemePreference::Dark), Some(Theme::Dark));
    }
}
