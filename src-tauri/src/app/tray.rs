/*!
 * SOURCE OF TRUTH KEYWORDS: tray, tray icon, tray menu, TrayIconBuilder, Open Echo, Start dictation, Stop dictation, Paste last, Pause hotkeys, Quit, tray tooltip, left click opens window
 * WHAT:  `create`: the notification-area icon with the registry's menu (registry/tray.rs). A left click opens the
 *        main window; the menu opens Echo, starts or stops dictation, pastes the last transcript, pauses the hotkeys
 *        and quits. The dictation item, the pause tick and the tooltip follow SessionStateChanged and
 *        HotkeyStatusChanged.
 * WHY:   Echo lives in the tray (01 F12, 02 §9): closing the window hides it, so the tray is how it comes back and how
 *        it quits. Every action goes through the same path as its other trigger: dictation is the session's Toggle
 *        input (the machine decides, as for a hotkey), paste-last the session's paste-last, the pause the shared
 *        HotkeyGate, the window the MainWindow port; actions from a click target the app the user left
 *        (TargetRule::LastExternal), since the click moved focus to the taskbar. The menu is Rust-side only: the
 *        webview gets no tray permission. Labels and state come from the events the windows receive, so the tray can
 *        never disagree with the pill or Settings, and a failed menu update is only logged. The menu opens on a right
 *        click (Windows convention), a left click opens the window. tray-icon re-adds the icon itself when explorer
 *        restarts (TaskbarCreated).
 * WHERE: `create` is called once by app::run on RunEvent::Ready, after the CommandCtx is managed.
 */

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::{
    AppHandle, Manager, Runtime,
    menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};
use tauri_specta::Event;

use crate::{
    ipc::CommandCtx,
    registry::tray::{TRAY_MENU, dictation_item, tooltip},
    types::{
        HotkeyStatus, HotkeyStatusChanged, SessionStateChanged, SessionStatus, SessionUiInput,
        TargetRule, TrayAction, TrayItemSpec,
    },
};

/// Id of Echo's tray icon.
const TRAY_ID: &str = "echo";

/// The parts of the tray that change after it is built.
struct LiveItems<R: Runtime> {
    tray: TrayIcon<R>,
    dictation: Option<MenuItem<R>>,
    pause: Option<CheckMenuItem<R>>,
}

impl<R: Runtime> Clone for LiveItems<R> {
    fn clone(&self) -> Self {
        Self {
            tray: self.tray.clone(),
            dictation: self.dictation.clone(),
            pause: self.pause.clone(),
        }
    }
}

/// Builds the tray icon and its menu; the icon stays for the life of the app.
pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let mut dictation = None;
    let mut pause = None;
    let mut items: Vec<Box<dyn IsMenuItem<R>>> = Vec::new();
    for spec in TRAY_MENU {
        if spec.separated {
            items.push(Box::new(PredefinedMenuItem::separator(app)?));
        }
        items.push(menu_item(app, spec, &mut dictation, &mut pause)?);
    }
    let refs: Vec<&dyn IsMenuItem<R>> = items.iter().map(AsRef::as_ref).collect();
    let menu = Menu::with_items(app, &refs)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Echo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_icon_event);
    match app.default_window_icon() {
        Some(icon) => builder = builder.icon(icon.clone()),
        None => tracing::error!("no app icon for the tray; the tray icon will be blank"),
    }
    let tray = builder.build(app)?;

    let live = LiveItems {
        tray,
        dictation,
        pause,
    };
    follow_state(app, &live);
    Ok(())
}

/// One menu item for `spec`; the dictation and pause items are kept for later updates.
fn menu_item<R: Runtime>(
    app: &AppHandle<R>,
    spec: &TrayItemSpec,
    dictation: &mut Option<MenuItem<R>>,
    pause: &mut Option<CheckMenuItem<R>>,
) -> tauri::Result<Box<dyn IsMenuItem<R>>> {
    let id = spec.action.id();
    if spec.check {
        let item = CheckMenuItem::with_id(app, id, spec.label.as_str(), true, false, None::<&str>)?;
        if spec.action == TrayAction::PauseHotkeys {
            *pause = Some(item.clone());
        }
        return Ok(Box::new(item));
    }
    let item = MenuItem::with_id(app, id, spec.label.as_str(), true, None::<&str>)?;
    if spec.action == TrayAction::ToggleDictation {
        *dictation = Some(item.clone());
    }
    Ok(Box::new(item))
}

/**
 * SOURCE OF TRUTH KEYWORDS: tray menu event, tray action dispatch, run tray action
 * WHAT:  Carries out the clicked item's TrayAction through the path its other trigger uses.
 * WHY:   One arm per registry action; a failure is logged (the session and the ports toast what the user must
 *        know), never a panic on the event loop.
 * WHERE: The tray's menu handler (registered by `create`).
 */
fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let Some(action) = TrayAction::from_id(event.id().as_ref()) else {
        return;
    };
    if action == TrayAction::Quit {
        tracing::info!("quit from the tray");
        app.exit(0);
        return;
    }
    let Some(ctx) = app.try_state::<CommandCtx>() else {
        tracing::error!("a tray action ran before the command context was managed");
        return;
    };
    let result = match action {
        TrayAction::OpenMain => ctx.main_window().show(),
        TrayAction::ToggleDictation => ctx.session().ui_input(SessionUiInput::Toggle),
        TrayAction::PasteLast => ctx.session().request_paste_last(TargetRule::LastExternal),
        TrayAction::PauseHotkeys => {
            let paused = ctx.hotkey_gate().status().paused;
            ctx.hotkey_gate().set_paused(!paused);
            Ok(())
        }
        TrayAction::Quit => Ok(()),
    };
    if let Err(error) = result {
        tracing::warn!(?action, detail = error.detail(), "a tray action failed");
    }
}

/// A left click (on release) opens the main window.
fn on_icon_event<R: Runtime>(tray: &TrayIcon<R>, event: TrayIconEvent) {
    let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    else {
        return;
    };
    let Some(ctx) = tray.app_handle().try_state::<CommandCtx>() else {
        return;
    };
    if let Err(error) = ctx.main_window().show() {
        tracing::warn!(
            detail = error.detail(),
            "the main window could not be shown from the tray"
        );
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: tray follows session, tray follows hotkey pause, tray tooltip update
 * WHAT:  Sets the dictation item, the pause tick and the tooltip now, and again on every SessionStateChanged and
 *        HotkeyStatusChanged.
 * WHY:   The events are the ones the windows receive, so the tray reads the same state; the last status of each is
 *        kept in the closures' shared cell so the tooltip combines both.
 * WHERE: `create`.
 */
fn follow_state<R: Runtime>(app: &AppHandle<R>, live: &LiveItems<R>) {
    let hotkeys = app
        .try_state::<CommandCtx>()
        .map(|ctx| ctx.hotkey_gate().status())
        .unwrap_or_default();
    let state = Arc::new(Mutex::new((SessionStatus::Idle, hotkeys)));
    render(live, *state.lock());

    let (session_live, session_state) = (live.clone(), Arc::clone(&state));
    SessionStateChanged::listen_any(app, move |event| {
        let now = {
            let mut shared = session_state.lock();
            shared.0 = event.payload.0.status;
            *shared
        };
        render(&session_live, now);
    });
    let (hotkeys_live, hotkeys_state) = (live.clone(), state);
    HotkeyStatusChanged::listen_any(app, move |event| {
        let now = {
            let mut shared = hotkeys_state.lock();
            shared.1 = event.payload.0;
            *shared
        };
        render(&hotkeys_live, now);
    });
}

/// Applies `status` and `hotkeys` to the menu and the tooltip.
fn render<R: Runtime>(live: &LiveItems<R>, (status, hotkeys): (SessionStatus, HotkeyStatus)) {
    let mut failures = Vec::new();
    if let Some(item) = &live.dictation {
        let (label, enabled) = dictation_item(status);
        failures.extend(item.set_text(label).err());
        failures.extend(item.set_enabled(enabled).err());
    }
    if let Some(item) = &live.pause {
        failures.extend(item.set_checked(hotkeys.paused).err());
    }
    failures.extend(live.tray.set_tooltip(Some(tooltip(status, hotkeys))).err());
    for error in failures {
        tracing::warn!(%error, "the tray could not be updated");
    }
}
