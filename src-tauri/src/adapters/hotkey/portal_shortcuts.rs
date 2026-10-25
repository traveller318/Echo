/*!
 * SOURCE OF TRUTH KEYWORDS: PortalGlobalShortcuts, xdg-desktop-portal GlobalShortcuts, Wayland hotkeys, Activated Deactivated, preferred trigger, BindShortcuts, portal session rebuild, GNOME no release events
 * WHAT:  PortalGlobalShortcuts: HotkeyService over the `org.freedesktop.portal.GlobalShortcuts` portal, the one
 *        sanctioned way for a Wayland client to receive keys it is not focused for. `register` / `unregister` change
 *        the table of wanted shortcuts; a worker thread (its own single-thread runtime) turns every change into a
 *        fresh portal session with the whole table bound, and forwards `Activated` as Pressed and `Deactivated` as
 *        Released to the sink.
 * WHY:   The portal lets the user, not the app, own the key: Echo only suggests a trigger (`CTRL+ALT+space`) and the
 *        desktop shows its own confirmation dialog the first time; what the user picks there is what fires. Echo never
 *        sees a key it was not given, which is the point on Wayland. A portal session can add shortcuts but never
 *        drop one, so a removed hotkey (the tray's Pause, a rebind) needs a new session; the desktop remembers the
 *        accepted ones per app, so that asks nothing again. Bare modifier chords (Ctrl+Alt) and keys with no modifier
 *        (Esc) cannot be portal shortcuts, and a global Esc would steal the key from every app for as long as the
 *        session lives, so both are refused as `Invalid` (the session's Esc is optional, the take still stops from
 *        the pill and the tray). Hold-to-talk needs the `Deactivated` signal, which only some desktops send (KDE
 *        Plasma and Hyprland do; GNOME's backend does not), so `caps().supports_release` follows the desktop and
 *        Settings falls back to toggle mode when it is false. `register` returns once the portal answered
 *        CreateSession (it never waits for the user's dialog); a missing portal is therefore an error the caller
 *        logs, and Echo runs on without hotkeys.
 * WHERE: Built by app/platform.rs on Linux for a Wayland session; the sink is the session actor's hotkey input.
 */

use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};

use ashpd::desktop::{
    Session,
    global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut},
};
use futures_util::StreamExt;
use parking_lot::Mutex;
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

use super::chords::chord::{Chord, Modifiers, parse};
use crate::{
    adapters::linux::register_app,
    ports::{EventSink, HotkeyService},
    types::{
        AppError, HotkeyCaps, HotkeyEvent, HotkeyId, HotkeyIssue, KeyState, PortError, PortResult,
        Shortcut,
    },
};

/// Desktops whose portal backend reports `Deactivated`, so a shortcut can be held.
const RELEASE_DESKTOPS: &[&str] = &["KDE", "Hyprland"];

/// How long `register` waits for the portal to answer CreateSession.
const SESSION_TIMEOUT: Duration = Duration::from_secs(10);

/// One shortcut as the portal is asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Wanted {
    id: HotkeyId,
    description: String,
    trigger: Option<String>,
}

type SharedSink = Arc<Mutex<Option<Arc<dyn EventSink<HotkeyEvent>>>>>;

pub struct PortalGlobalShortcuts {
    sink: SharedSink,
    app_id: String,
    labels: Vec<(HotkeyId, String)>,
    release: bool,
    table: Mutex<BTreeMap<HotkeyId, (Shortcut, Chord)>>,
    worker: Mutex<Option<Worker>>,
}

struct Worker {
    wanted: tokio_mpsc::UnboundedSender<Vec<Wanted>>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl PortalGlobalShortcuts {
    /// `app_id` names Echo to the portal; `labels` are the descriptions the desktop shows for each hotkey; `desktop` is
    /// `XDG_CURRENT_DESKTOP`.
    pub fn new(app_id: &str, labels: Vec<(HotkeyId, String)>, desktop: &str) -> Self {
        Self {
            sink: SharedSink::default(),
            app_id: app_id.to_owned(),
            labels,
            release: desktop.split(':').any(|part| {
                RELEASE_DESKTOPS
                    .iter()
                    .any(|name| part.eq_ignore_ascii_case(name))
            }),
            table: Mutex::default(),
            worker: Mutex::default(),
        }
    }

    fn wanted(&self, table: &BTreeMap<HotkeyId, (Shortcut, Chord)>) -> Vec<Wanted> {
        table
            .iter()
            .map(|(id, (_, chord))| Wanted {
                id: id.clone(),
                description: self
                    .labels
                    .iter()
                    .find(|(label_id, _)| label_id == id)
                    .map_or_else(|| id.to_string(), |(_, label)| label.clone()),
                trigger: trigger_of(chord),
            })
            .collect()
    }

    /// Starts the worker unless it runs.
    fn ensure_worker(&self) -> PortResult<()> {
        let mut worker = self.worker.lock();
        if worker.is_none() {
            *worker = Some(Worker::start(Arc::clone(&self.sink), self.app_id.clone())?);
        }
        Ok(())
    }

    /// Sends the table to the worker, or stops it when nothing is wanted.
    fn apply(&self, table: &BTreeMap<HotkeyId, (Shortcut, Chord)>) {
        let mut worker = self.worker.lock();
        if table.is_empty() {
            worker.take();
        } else if let Some(running) = worker.as_ref() {
            drop(running.wanted.send(self.wanted(table)));
        }
    }
}

impl HotkeyService for PortalGlobalShortcuts {
    fn caps(&self) -> HotkeyCaps {
        HotkeyCaps {
            supports_release: self.release,
            supports_modifier_only: false,
        }
    }

    fn listen(&self, sink: Arc<dyn EventSink<HotkeyEvent>>) -> PortResult<()> {
        *self.sink.lock() = Some(sink);
        Ok(())
    }

    fn register(&self, id: &HotkeyId, shortcut: &Shortcut) -> PortResult<()> {
        let chord = parse(shortcut)?;
        if chord.is_modifier_only() {
            return Err(invalid(
                shortcut,
                "the desktop needs a key besides the modifiers",
            ));
        }
        if chord.modifiers == Modifiers::NONE {
            return Err(invalid(
                shortcut,
                "a global shortcut needs a modifier, or it would take the key from every app",
            ));
        }
        let mut table = self.table.lock();
        if let Some((owner, _)) = table
            .iter()
            .find(|(bound, (_, other))| *bound != id && *other == chord)
        {
            return Err(PortError::new(AppError::Hotkey {
                reason: HotkeyIssue::Conflict,
            })
            .with_detail(format!("{shortcut} is already Echo's {owner} hotkey")));
        }
        let previous = table.insert(id.clone(), (shortcut.clone(), chord));
        if let Err(error) = self.ensure_worker() {
            match previous {
                Some(previous) => table.insert(id.clone(), previous),
                None => table.remove(id),
            };
            return Err(error);
        }
        self.apply(&table);
        Ok(())
    }

    fn unregister(&self, id: &HotkeyId) -> PortResult<()> {
        let mut table = self.table.lock();
        if table.remove(id).is_some() {
            self.apply(&table);
        }
        Ok(())
    }

    fn refresh(&self) -> PortResult<()> {
        // A portal session survives sleep and unlock; nothing needs registering again.
        Ok(())
    }
}

fn invalid(shortcut: &Shortcut, why: &str) -> PortError {
    PortError::new(AppError::Hotkey {
        reason: HotkeyIssue::Invalid,
    })
    .with_detail(format!("{shortcut}: {why}"))
}

/// The portal's trigger text for `chord` (`CTRL+ALT+space`), or None for a key it has no keysym name for.
fn trigger_of(chord: &Chord) -> Option<String> {
    let key = keysym_name(chord.key?)?;
    let mut parts = Vec::new();
    for (modifier, name) in [
        (Modifiers::CTRL, "CTRL"),
        (Modifiers::ALT, "ALT"),
        (Modifiers::SHIFT, "SHIFT"),
        (Modifiers::WIN, "LOGO"),
    ] {
        if chord.modifiers.contains(modifier) {
            parts.push(name.to_owned());
        }
    }
    parts.push(key);
    Some(parts.join("+"))
}

/// The XKB keysym name of a virtual key.
fn keysym_name(vk: u16) -> Option<String> {
    let name = match vk {
        0x41..=0x5A | 0x30..=0x39 => {
            return char::from_u32(u32::from(vk)).map(|c| c.to_ascii_lowercase().to_string());
        }
        0x70..=0x87 => return Some(format!("F{}", vk - 0x6F)),
        0x60..=0x69 => return Some(format!("KP_{}", vk - 0x60)),
        0x20 => "space",
        0x0D => "Return",
        0x09 => "Tab",
        0x1B => "Escape",
        0x08 => "BackSpace",
        0x2E => "Delete",
        0x2D => "Insert",
        0x24 => "Home",
        0x23 => "End",
        0x21 => "Prior",
        0x22 => "Next",
        0x26 => "Up",
        0x28 => "Down",
        0x25 => "Left",
        0x27 => "Right",
        0xBC => "comma",
        0xBE => "period",
        0xBF => "slash",
        0xBA => "semicolon",
        0xDE => "apostrophe",
        0xDB => "bracketleft",
        0xDD => "bracketright",
        0xDC => "backslash",
        0xBD => "minus",
        0xBB => "equal",
        0xC0 => "grave",
        _ => return None,
    };
    Some(name.to_owned())
}

impl Worker {
    /// Starts the thread and waits until the portal answered the first CreateSession.
    fn start(sink: SharedSink, app_id: String) -> PortResult<Self> {
        let (wanted, requests) = tokio_mpsc::unbounded_channel();
        let (stop, stopped) = oneshot::channel();
        let (ready, answered) = mpsc::channel::<Result<(), String>>();
        let thread = thread::Builder::new()
            .name("echo-portal-keys".to_owned())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        drop(ready.send(Err(error.to_string())));
                        return;
                    }
                };
                runtime.block_on(serve(sink, app_id, requests, ready, stopped));
            })
            .map_err(|error| unavailable(&error.to_string()))?;
        match answered.recv_timeout(SESSION_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                wanted,
                stop: Some(stop),
                thread: Some(thread),
            }),
            Ok(Err(detail)) => {
                drop(thread.join());
                Err(unavailable(&detail))
            }
            Err(_) => {
                drop(stop);
                Err(unavailable("the portal did not answer in time"))
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::error!("the portal shortcut thread panicked");
        }
    }
}

fn unavailable(detail: &str) -> PortError {
    PortError::new(AppError::Hotkey {
        reason: HotkeyIssue::Invalid,
    })
    .with_detail(format!(
        "the desktop's global shortcut portal is unavailable: {detail}"
    ))
}

/// The worker body: proves the portal answers, then rebinds on every request and relays the signals.
async fn serve(
    sink: SharedSink,
    app_id: String,
    mut requests: tokio_mpsc::UnboundedReceiver<Vec<Wanted>>,
    ready: mpsc::Sender<Result<(), String>>,
    stopped: oneshot::Receiver<()>,
) {
    register_app(&app_id).await;
    let portal = match GlobalShortcuts::new().await {
        Ok(portal) => portal,
        Err(error) => {
            drop(ready.send(Err(error.to_string())));
            return;
        }
    };
    let (mut activated, mut deactivated) = match (
        portal.receive_activated().await,
        portal.receive_deactivated().await,
    ) {
        (Ok(activated), Ok(deactivated)) => (Box::pin(activated), Box::pin(deactivated)),
        (Err(error), _) | (_, Err(error)) => {
            drop(ready.send(Err(error.to_string())));
            return;
        }
    };
    drop(ready.send(Ok(())));
    let emit = |id: &str, state: KeyState| {
        if let Some(sink) = sink.lock().as_ref() {
            sink.emit(HotkeyEvent {
                id: HotkeyId::from(id.to_owned()),
                state,
            });
        }
    };
    let mut session: Option<Session<GlobalShortcuts>> = None;
    tokio::pin!(stopped);
    loop {
        tokio::select! {
            _ = &mut stopped => break,
            Some(wanted) = requests.recv() => {
                // Only the newest table matters if several changes queued up.
                let mut wanted = wanted;
                while let Ok(newer) = requests.try_recv() {
                    wanted = newer;
                }
                rebind(&portal, &mut session, &wanted).await;
            }
            Some(signal) = activated.next() => emit(signal.shortcut_id(), KeyState::Pressed),
            Some(signal) = deactivated.next() => emit(signal.shortcut_id(), KeyState::Released),
            else => break,
        }
    }
    if let Some(session) = session
        && let Err(error) = session.close().await
    {
        tracing::debug!(%error, "the shortcut session was already closed");
    }
}

/// Replaces the portal session with a new one that has exactly `wanted` bound.
async fn rebind(
    portal: &GlobalShortcuts,
    session: &mut Option<Session<GlobalShortcuts>>,
    wanted: &[Wanted],
) {
    if let Some(old) = session.take()
        && let Err(error) = old.close().await
    {
        tracing::debug!(%error, "the previous shortcut session was already closed");
    }
    let created = match portal.create_session(Default::default()).await {
        Ok(created) => created,
        Err(error) => {
            tracing::warn!(%error, "a shortcut session could not be created; hotkeys are off");
            return;
        }
    };
    let shortcuts: Vec<NewShortcut> = wanted
        .iter()
        .map(|want| {
            NewShortcut::new(want.id.as_str(), want.description.as_str())
                .preferred_trigger(want.trigger.as_deref())
        })
        .collect();
    match portal
        .bind_shortcuts(&created, &shortcuts, None, BindShortcutsOptions::default())
        .await
    {
        Ok(request) => match request.response() {
            Ok(bound) => tracing::info!(
                hotkeys = bound.shortcuts().len(),
                "the desktop bound Echo's global shortcuts"
            ),
            Err(error) => {
                tracing::warn!(%error, "the desktop did not bind Echo's shortcuts; set them in its keyboard settings")
            }
        },
        Err(error) => tracing::warn!(%error, "global shortcuts could not be bound"),
    }
    *session = Some(created);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &'static str) -> Chord {
        parse(&Shortcut::from_static(text)).unwrap()
    }

    #[test]
    fn triggers_use_the_portal_spelling() {
        assert_eq!(
            trigger_of(&chord("Ctrl+Alt+Space")).as_deref(),
            Some("CTRL+ALT+space")
        );
        assert_eq!(
            trigger_of(&chord("ctrl+shift+v")).as_deref(),
            Some("CTRL+SHIFT+v")
        );
        assert_eq!(trigger_of(&chord("Super+F9")).as_deref(), Some("LOGO+F9"));
        assert_eq!(
            trigger_of(&chord("Ctrl+Alt+Numpad5")).as_deref(),
            Some("CTRL+ALT+KP_5")
        );
        assert_eq!(trigger_of(&chord("Ctrl+Alt")), None);
    }

    #[test]
    fn release_support_follows_the_desktop() {
        let caps =
            |desktop| PortalGlobalShortcuts::new("app.echo.test", Vec::new(), desktop).caps();
        assert!(caps("KDE").supports_release);
        assert!(caps("Hyprland").supports_release);
        assert!(!caps("ubuntu:GNOME").supports_release);
        assert!(!caps("").supports_release);
        assert!(!caps("KDE").supports_modifier_only);
    }

    #[test]
    fn bare_modifier_and_unmodified_keys_are_refused_before_any_portal_call() {
        let portal = PortalGlobalShortcuts::new("app.echo.test", Vec::new(), "KDE");
        let id = HotkeyId::from_static("record");
        for text in ["Ctrl+Alt", "Escape", "V", "F5"] {
            let error = portal
                .register(&id, &Shortcut::from(text.to_owned()))
                .unwrap_err();
            assert_eq!(
                error.into_app_error(),
                AppError::Hotkey {
                    reason: HotkeyIssue::Invalid
                },
                "{text}"
            );
        }
        assert!(
            portal.worker.lock().is_none(),
            "nothing valid was asked for, so no portal session starts"
        );
    }
}
