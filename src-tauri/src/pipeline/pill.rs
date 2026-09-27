/*!
 * SOURCE OF TRUTH KEYWORDS: PillPresenter, PillTiming, pill window show hide, pill exit animation, pill_exited, pill hit areas, click-through polling, pill placement, pill always visible, pill drag, look_after_settings_change
 * WHAT:  PillPresenter: follows SessionStateChanged and the pill settings and keeps the pill window in step. It shows
 *        the pill (on the monitor of the window the user is typing in, or where the user dragged it) the moment a
 *        take leaves Idle, and also while Idle when `pill.visibility` is `always`; hides it once the page says its
 *        exit animation finished (or after a fallback delay); while the pill shows clickable areas, polls the
 *        pointer so clicks pass through everywhere else; and runs a drag the page asks for, moving the window with
 *        the cursor until the primary button is released. `look_after_settings_change` tells SettingsEffects when
 *        the pill page needs a new PillLook.
 * WHY:   The session actor owns recording state and knows nothing about windows; the presenter observes the same
 *        event stream the UI gets (through FanOut), so the window can never disagree with the view (02 §5). Showing
 *        waits for nothing: the window is pre-created and shown without activation on the first non-Idle view, so
 *        "hotkey → pill visible" stays under 50 ms (02 §6.2). Hiding waits for the page's exit animation
 *        (`pill_exited`), because hiding first would cut it off; if the page never answers (it crashed, it is
 *        still loading) EXIT_FALLBACK hides it anyway. A take that starts while the pill is leaving cancels the hide
 *        and moves the pill to the new take's monitor; with an always-visible pill a take start re-places it there
 *        too, unless the user dragged it somewhere (`pill.movable` + `pill.position`), which always wins. A
 *        click-through window gets no pointer events, so only Rust can tell that the pointer reached a clickable
 *        area: the pointer is polled only while the pill shows one, and never otherwise. A drag is polled here too
 *        (window corner = cursor − grab offset) instead of using the system move loop, which activates the window
 *        and blocks the UI thread; its final corner is handed back to the command that stores it, since only the
 *        command layer writes settings with the right context. The overlay exists only once the windows do, so an
 *        always-visible pill first shows on `attached`. All window work runs on the presenter's own thread, so
 *        `emit` (called by the session actor) only queues a message and never waits for the window manager.
 *        Failures only cost looks and are logged; the take itself never depends on the pill.
 * WHERE: Spawned by app/bootstrap and wired into the event FanOut; `set_hit_areas`, `exited` and `drag` are called by
 *        ipc/commands/pill.rs (`pill_set_hit_areas`, `pill_exited`, `pill_drag`) from src/pill; `look_changed` by
 *        pipeline/settings_effects.rs; `attached` by app/windows.rs. Works through the OverlayWindow and
 *        ForegroundApp ports and reads the pill settings from SharedSettings (registry::settings::pill_look).
 */

use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use tokio::sync::oneshot;

use super::settings_store;
use crate::{
    ports::{EventSink, ForegroundApp, OverlayWindow},
    registry::{
        self,
        settings::{keys, values},
    },
    services::Db,
    types::{
        AppError, AppEvent, OverlayPlacement, OverlayRect, PillLook, PillVisibility, PortError,
        PortResult, ScreenPoint, ScreenRect, SessionStateChanged, SessionStatus, SettingsSnapshot,
        SharedSettings,
    },
};

/**
 * SOURCE OF TRUTH KEYWORDS: PillTiming, pointer poll interval, exit fallback, drag poll interval
 * WHAT:  How often the pointer is checked against the pill's clickable areas, how often a drag moves the window, and
 *        how long the pill may stay up after the session went Idle when the page never reports its exit animation.
 * WHY:   30 Hz makes a button feel instantly clickable while costing nothing measurable (a cursor read); a drag
 *        follows the cursor at 120 Hz so the pill does not trail behind it; the fallback outlasts the slowest exit
 *        spring (pillExit settles in well under a second, 04 §3.7).
 * WHERE: PillPresenter::spawn (DEFAULT in the app, shorter values in tests).
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PillTiming {
    pub pointer_poll: Duration,
    pub drag_poll: Duration,
    pub exit_fallback: Duration,
}

impl PillTiming {
    pub const DEFAULT: Self = Self {
        pointer_poll: Duration::from_millis(33),
        drag_poll: Duration::from_millis(8),
        exit_fallback: Duration::from_millis(1_500),
    };
}

/// Where a finished drag left the window's corner; None when the pill did not move (a click) or no drag ran.
pub type DragReply = oneshot::Sender<Option<ScreenPoint>>;

enum PillMessage {
    Status(SessionStatus),
    HitAreas(Vec<OverlayRect>),
    Exited,
    LookChanged,
    Attached,
    Drag(DragReply),
}

/// The way into the pill presenter's thread; clones share it.
#[derive(Clone)]
pub struct PillPresenter {
    inbox: Sender<PillMessage>,
}

impl PillPresenter {
    /// Starts the presenter thread; it ends when every PillPresenter is dropped.
    pub fn spawn(
        overlay: Arc<dyn OverlayWindow>,
        foreground: Arc<dyn ForegroundApp>,
        settings: SharedSettings,
        timing: PillTiming,
    ) -> PortResult<Self> {
        let (inbox, messages) = mpsc::channel();
        let worker = Worker {
            overlay,
            foreground,
            settings,
            timing,
            idle: true,
            shown: false,
            hide_at: None,
            areas: Vec::new(),
            taking_clicks: false,
            drag: None,
        };
        thread::Builder::new()
            .name(String::from("echo-pill"))
            .spawn(move || worker.run(&messages))
            .map_err(|error| {
                PortError::new(AppError::Internal)
                    .with_detail(format!("the pill thread could not start: {error}"))
            })?;
        Ok(Self { inbox })
    }

    /// The page's clickable areas are now at `areas` (CSS pixels of the pill page); empty when it shows none.
    pub fn set_hit_areas(&self, areas: Vec<OverlayRect>) {
        self.send(PillMessage::HitAreas(areas));
    }

    /// The page finished its exit animation: the window can go.
    pub fn exited(&self) {
        self.send(PillMessage::Exited);
    }

    /// The pill settings changed: show, hide or re-place the pill for them.
    pub fn look_changed(&self) {
        self.send(PillMessage::LookChanged);
    }

    /// The pill window exists now: an always-visible pill can show.
    pub fn attached(&self) {
        self.send(PillMessage::Attached);
    }

    /// Drags the pill with the cursor until the primary button is released; resolves to the window's new corner, or
    /// None when it did not move or cannot be dragged now (not shown, leaving, not movable, already dragging).
    pub fn drag(&self) -> oneshot::Receiver<Option<ScreenPoint>> {
        let (reply, answer) = oneshot::channel();
        self.send(PillMessage::Drag(reply));
        answer
    }

    fn send(&self, message: PillMessage) {
        // Fails only once the thread has ended at exit; nothing is left to update then.
        let _ = self.inbox.send(message);
    }
}

impl EventSink<AppEvent> for PillPresenter {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::SessionStateChanged(SessionStateChanged(view)) = event {
            self.send(PillMessage::Status(view.status));
        }
    }
}

/// The PillLook to announce after a settings write, or None when the write did not change it.
pub fn look_after_settings_change(
    before: &SettingsSnapshot,
    after: &SettingsSnapshot,
) -> Option<PillLook> {
    let look = registry::settings::pill_look(after);
    (registry::settings::pill_look(before) != look).then_some(look)
}

/**
 * SOURCE OF TRUTH KEYWORDS: remember_position, store dragged pill position, pill.position write
 * WHAT:  Stores where a drag left the pill's window corner as the hidden `pill.position` setting and announces it.
 * WHY:   The next show (a later take, a restart) puts the pill back there; the write goes through the one internal
 *        settings path, so it is validated against the registry and the cached snapshot equals the table.
 * WHERE: ipc/commands/pill.rs (`pill_drag`) with the corner PillPresenter::drag answered.
 */
pub fn remember_position(
    settings: &SharedSettings,
    db: &Db,
    events: &dyn EventSink<AppEvent>,
    corner: ScreenPoint,
) -> PortResult<()> {
    settings_store::store_internal(
        settings,
        db,
        events,
        &keys::PILL_POSITION,
        values::pill_position(corner),
    )
}

/// A drag in progress: the cursor's offset from the window corner when it began.
struct Drag {
    grab_x: i32,
    grab_y: i32,
    moved_to: Option<ScreenPoint>,
    reply: DragReply,
}

/// The presenter thread's state.
struct Worker {
    overlay: Arc<dyn OverlayWindow>,
    foreground: Arc<dyn ForegroundApp>,
    settings: SharedSettings,
    timing: PillTiming,
    /// The session is Idle (no take running).
    idle: bool,
    /// The window is up (possibly playing its exit animation).
    shown: bool,
    /// The pill is leaving: hide at this instant unless the page says it finished first.
    hide_at: Option<Instant>,
    /// The page's clickable areas.
    areas: Vec<OverlayRect>,
    /// Click-through is off because the pointer is on a clickable area.
    taking_clicks: bool,
    drag: Option<Drag>,
}

impl Worker {
    fn run(mut self, messages: &Receiver<PillMessage>) {
        loop {
            let message = match self.next_wake(Instant::now()) {
                None => match messages.recv() {
                    Ok(message) => Some(message),
                    Err(_) => break,
                },
                Some(wait) => match messages.recv_timeout(wait) {
                    Ok(message) => Some(message),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                },
            };
            if let Some(message) = message {
                self.handle(message);
            }
            self.tick(Instant::now());
        }
    }

    /// How long the thread may sleep: until the next drag step, pointer check or fallback hide; None waits for a
    /// message.
    fn next_wake(&self, now: Instant) -> Option<Duration> {
        let poll = if self.drag.is_some() {
            Some(self.timing.drag_poll)
        } else {
            (self.shown && !self.areas.is_empty()).then_some(self.timing.pointer_poll)
        };
        let hide = self.hide_at.map(|at| at.saturating_duration_since(now));
        match (poll, hide) {
            (Some(poll), Some(hide)) => Some(poll.min(hide)),
            (poll, hide) => poll.or(hide),
        }
    }

    fn look(&self) -> PillLook {
        registry::settings::pill_look(&self.settings.current())
    }

    /// The pill belongs on screen now: a take runs, or the user keeps it up all the time.
    fn wanted(&self) -> bool {
        !self.idle || self.look().visibility == PillVisibility::Always
    }

    fn handle(&mut self, message: PillMessage) {
        match message {
            PillMessage::Status(SessionStatus::Idle) => {
                self.idle = true;
                if !self.wanted() {
                    self.leave();
                }
            }
            PillMessage::Status(_) => {
                // A take starting places the pill again: it appears, stops leaving, or follows the user's monitor.
                let starting = self.idle || !self.shown || self.hide_at.is_some();
                self.idle = false;
                self.hide_at = None;
                if starting {
                    self.show();
                }
            }
            PillMessage::HitAreas(areas) => {
                self.areas = areas;
                if self.areas.is_empty() {
                    self.take_clicks(false);
                }
            }
            PillMessage::Exited => {
                if self.hide_at.is_some() {
                    self.hide();
                }
            }
            PillMessage::LookChanged => {
                if !self.look().movable {
                    self.finish_drag();
                }
                if self.wanted() {
                    // Shows an always-visible pill, or moves a shown one where the new settings put it.
                    self.hide_at = None;
                    self.show();
                } else {
                    self.leave();
                }
            }
            PillMessage::Attached => {
                if self.wanted() {
                    self.show();
                }
            }
            PillMessage::Drag(reply) => self.start_drag(reply),
        }
    }

    fn tick(&mut self, now: Instant) {
        if self.hide_at.is_some_and(|at| at <= now) {
            self.hide();
        }
        if self.drag.is_some() {
            self.step_drag();
        }
        if self.shown && !self.areas.is_empty() {
            let over = self
                .overlay
                .pointer_over(&self.areas)
                .unwrap_or_else(|error| {
                    tracing::debug!(detail = error.detail(), "the pointer could not be read");
                    false
                });
            // A drag keeps clicks while the pointer races ahead of the window.
            self.take_clicks(over || self.drag.is_some());
        }
    }

    /// Starts the exit: the page animates the pill out and reports it, or the fallback hides it.
    fn leave(&mut self) {
        if self.shown && self.hide_at.is_none() {
            self.hide_at = Some(Instant::now() + self.timing.exit_fallback);
        }
    }

    /// Shows the pill where the user dragged it, or at the bottom centre of the monitor of the window that has
    /// focus now (the take's target). A drag in progress is never interrupted by a re-placement.
    fn show(&mut self) {
        if self.drag.is_some() {
            return;
        }
        let placement = match registry::settings::pill_dragged_position(&self.settings.current()) {
            Some(corner) => OverlayPlacement::At(corner),
            None => OverlayPlacement::BottomCentre(self.target_work_area()),
        };
        match self.overlay.show(placement) {
            Ok(()) => self.shown = true,
            Err(error) => tracing::warn!(
                detail = error.detail(),
                "the pill could not be shown; the take goes on without it"
            ),
        }
    }

    fn target_work_area(&self) -> Option<ScreenRect> {
        match self.foreground.current() {
            Ok(target) => target.and_then(|target| target.work_area),
            Err(error) => {
                tracing::debug!(
                    detail = error.detail(),
                    "the focused window could not be read; the pill uses the primary monitor"
                );
                None
            }
        }
    }

    fn hide(&mut self) {
        self.finish_drag();
        self.hide_at = None;
        self.areas.clear();
        self.take_clicks(false);
        if let Err(error) = self.overlay.hide() {
            tracing::warn!(detail = error.detail(), "the pill could not be hidden");
        }
        self.shown = false;
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: start_drag, pill drag start, grab offset, drag refused
     * WHAT:  Begins a drag: remembers where the cursor holds the window (cursor − window corner).
     * WHY:   Only a shown, staying, movable pill drags, and one drag at a time; anything else answers None at once so
     *        the command never waits for nothing.
     * WHERE: PillMessage::Drag (from `pill_drag`).
     */
    fn start_drag(&mut self, reply: DragReply) {
        if !self.shown || self.hide_at.is_some() || self.drag.is_some() || !self.look().movable {
            let _ = reply.send(None);
            return;
        }
        match self.overlay.origin().and_then(|origin| {
            self.overlay.cursor().map(|cursor| {
                (
                    cursor.x.saturating_sub(origin.x),
                    cursor.y.saturating_sub(origin.y),
                )
            })
        }) {
            Ok((grab_x, grab_y)) => {
                self.drag = Some(Drag {
                    grab_x,
                    grab_y,
                    moved_to: None,
                    reply,
                });
            }
            Err(error) => {
                tracing::debug!(detail = error.detail(), "the pill drag could not start");
                let _ = reply.send(None);
            }
        }
    }

    /// Moves the window under the cursor while the primary button is held; ends the drag once it is released.
    fn step_drag(&mut self) {
        let held = self.overlay.primary_button_down().unwrap_or_else(|error| {
            tracing::debug!(
                detail = error.detail(),
                "the mouse button could not be read"
            );
            false
        });
        if !held {
            self.finish_drag();
            return;
        }
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        let cursor = match self.overlay.cursor() {
            Ok(cursor) => cursor,
            Err(error) => {
                tracing::debug!(detail = error.detail(), "the cursor could not be read");
                return;
            }
        };
        let corner = ScreenPoint {
            x: cursor.x.saturating_sub(drag.grab_x),
            y: cursor.y.saturating_sub(drag.grab_y),
        };
        if drag.moved_to == Some(corner) {
            return;
        }
        match self.overlay.move_to(corner) {
            Ok(()) => drag.moved_to = Some(corner),
            Err(error) => tracing::debug!(detail = error.detail(), "the pill could not be moved"),
        }
    }

    /// Ends a drag in progress and answers with where it left the window.
    fn finish_drag(&mut self) {
        if let Some(drag) = self.drag.take() {
            // The command may have given up waiting (the page closed); the window stays where it is either way.
            let _ = drag.reply.send(drag.moved_to);
        }
    }

    /// Switches click-through off while the pointer is on a clickable area, back on otherwise.
    fn take_clicks(&mut self, take: bool) {
        if take == self.taking_clicks {
            return;
        }
        match self.overlay.set_click_through(!take) {
            Ok(()) => self.taking_clicks = take,
            Err(error) => tracing::debug!(
                detail = error.detail(),
                "the pill's click-through could not change"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::{FakeForegroundApp, FakeOverlayWindow, OverlayCall, RecordingSink},
        types::{PillStyle, SessionView, SettingKey, SettingValue, SettingsChanged, StaticStr},
    };

    const TIMING: PillTiming = PillTiming {
        pointer_poll: Duration::from_millis(5),
        drag_poll: Duration::from_millis(2),
        exit_fallback: Duration::from_millis(150),
    };
    const WAIT: Duration = Duration::from_secs(5);

    fn view(status: SessionStatus) -> AppEvent {
        SessionStateChanged(SessionView {
            status,
            ..SessionView::IDLE
        })
        .into()
    }

    fn laptop() -> ScreenRect {
        ScreenRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1032,
        }
    }

    fn on_laptop() -> OverlayCall {
        OverlayCall::Show(OverlayPlacement::BottomCentre(Some(laptop())))
    }

    fn snapshot(stored: &[(SettingKey, SettingValue)]) -> SettingsSnapshot {
        registry::settings::resolve(stored.iter().cloned())
    }

    fn always() -> (SettingKey, SettingValue) {
        (
            keys::PILL_VISIBILITY,
            SettingValue::Enum(StaticStr::new(PillVisibility::Always.as_str())),
        )
    }

    fn movable() -> (SettingKey, SettingValue) {
        (keys::PILL_MOVABLE, SettingValue::Bool(true))
    }

    struct Rig {
        overlay: Arc<FakeOverlayWindow>,
        foreground: Arc<FakeForegroundApp>,
        settings: SharedSettings,
        pill: PillPresenter,
    }

    impl Rig {
        fn new() -> Self {
            Self::with(&[])
        }

        fn with(stored: &[(SettingKey, SettingValue)]) -> Self {
            let overlay = Arc::new(FakeOverlayWindow::default());
            let foreground = Arc::new(FakeForegroundApp::default());
            let mut target = FakeForegroundApp::target("notepad.exe", false);
            target.work_area = Some(laptop());
            foreground.set(Some(target));
            let settings = SharedSettings::new(snapshot(stored));
            let pill = PillPresenter::spawn(
                overlay.clone(),
                foreground.clone(),
                settings.clone(),
                TIMING,
            )
            .unwrap();
            Self {
                overlay,
                foreground,
                settings,
                pill,
            }
        }

        fn change(&self, stored: &[(SettingKey, SettingValue)]) {
            self.settings.replace(snapshot(stored));
            self.pill.look_changed();
        }

        fn calls_until(&self, done: impl Fn(&[OverlayCall]) -> bool) -> Vec<OverlayCall> {
            self.overlay.wait_for(WAIT, done)
        }

        fn drag(&self) -> Option<ScreenPoint> {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(self.pill.drag())
                .unwrap()
        }
    }

    fn count(calls: &[OverlayCall], call: &OverlayCall) -> usize {
        calls.iter().filter(|seen| *seen == call).count()
    }

    #[test]
    fn a_take_shows_the_pill_once_on_the_targets_monitor_and_the_exit_hides_it() {
        let rig = Rig::new();
        rig.pill.attached();
        for status in [
            SessionStatus::Arming,
            SessionStatus::Recording,
            SessionStatus::Finalizing,
            SessionStatus::Delivering,
            SessionStatus::Done,
        ] {
            rig.pill.emit(view(status));
        }
        rig.pill.emit(view(SessionStatus::Idle));
        rig.pill.exited();
        let calls = rig.calls_until(|calls| calls.contains(&OverlayCall::Hide));
        assert_eq!(calls, [on_laptop(), OverlayCall::Hide]);
    }

    #[test]
    fn the_fallback_hides_a_pill_whose_page_never_answers() {
        let rig = Rig::new();
        rig.pill.emit(view(SessionStatus::Arming));
        rig.pill.emit(view(SessionStatus::Idle));
        let started = Instant::now();
        let calls = rig.calls_until(|calls| calls.contains(&OverlayCall::Hide));
        assert!(started.elapsed() >= TIMING.exit_fallback - Duration::from_millis(20));
        assert_eq!(calls.last(), Some(&OverlayCall::Hide));
        // A late exit report after the hide changes nothing.
        rig.pill.exited();
        rig.pill.emit(view(SessionStatus::Idle));
        thread::sleep(Duration::from_millis(30));
        assert_eq!(count(&rig.overlay.calls(), &OverlayCall::Hide), 1);
    }

    #[test]
    fn a_take_that_starts_while_the_pill_leaves_keeps_it_and_moves_it() {
        let rig = Rig::new();
        rig.pill.emit(view(SessionStatus::Recording));
        rig.calls_until(|calls| !calls.is_empty());
        rig.pill.emit(view(SessionStatus::Idle));
        let external = ScreenRect {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1400,
        };
        let mut target = FakeForegroundApp::target("code.exe", false);
        target.work_area = Some(external);
        rig.foreground.set(Some(target));
        rig.pill.emit(view(SessionStatus::Arming));
        // The exit report of the previous take arrives after the new take began: it must not hide the pill.
        rig.pill.exited();
        let calls = rig.calls_until(|calls| calls.len() >= 2);
        thread::sleep(TIMING.exit_fallback + Duration::from_millis(50));
        assert_eq!(
            calls[..2],
            [
                on_laptop(),
                OverlayCall::Show(OverlayPlacement::BottomCentre(Some(external)))
            ]
        );
        assert!(!rig.overlay.calls().contains(&OverlayCall::Hide));
    }

    #[test]
    fn clicks_reach_the_pill_only_while_the_pointer_is_on_a_button() {
        let rig = Rig::new();
        rig.pill.emit(view(SessionStatus::Recording));
        let stop = OverlayRect {
            x: 150,
            y: 28,
            width: 28,
            height: 28,
        };
        rig.pill.set_hit_areas(vec![stop]);
        thread::sleep(Duration::from_millis(30));
        assert!(
            !rig.overlay
                .calls()
                .contains(&OverlayCall::ClickThrough(false))
        );
        rig.overlay.set_pointer_over(true);
        rig.calls_until(|calls| calls.contains(&OverlayCall::ClickThrough(false)));
        rig.overlay.set_pointer_over(false);
        let calls = rig.calls_until(|calls| calls.contains(&OverlayCall::ClickThrough(true)));
        assert_eq!(
            count(&calls, &OverlayCall::ClickThrough(false)),
            1,
            "changes only, no repeats"
        );

        // The buttons going away restores click-through at once.
        rig.overlay.set_pointer_over(true);
        rig.calls_until(|calls| count(calls, &OverlayCall::ClickThrough(false)) == 2);
        rig.pill.set_hit_areas(Vec::new());
        rig.calls_until(|calls| count(calls, &OverlayCall::ClickThrough(true)) == 2);
    }

    #[test]
    fn a_pill_that_cannot_show_is_retried_and_an_unknown_target_uses_the_primary_monitor() {
        let rig = Rig::new();
        rig.overlay
            .fail_next_show(PortError::new(AppError::Internal));
        rig.pill.emit(view(SessionStatus::Arming));
        rig.foreground.set(None);
        rig.pill.emit(view(SessionStatus::Recording));
        let calls = rig.calls_until(|calls| !calls.is_empty());
        assert_eq!(
            calls,
            [OverlayCall::Show(OverlayPlacement::BottomCentre(None))]
        );
    }

    #[test]
    fn an_always_visible_pill_shows_once_attached_stays_between_takes_and_follows_the_next_take() {
        let rig = Rig::with(&[always()]);
        rig.pill.attached();
        rig.calls_until(|calls| calls == [on_laptop()]);
        rig.pill.emit(view(SessionStatus::Recording));
        rig.calls_until(|calls| calls.len() == 2);
        rig.pill.emit(view(SessionStatus::Idle));
        rig.pill.exited();
        thread::sleep(TIMING.exit_fallback + Duration::from_millis(50));
        let calls = rig.overlay.calls();
        assert_eq!(calls, [on_laptop(), on_laptop()], "never hidden");
    }

    #[test]
    fn switching_visibility_shows_or_retires_an_idle_pill() {
        let rig = Rig::new();
        rig.pill.attached();
        thread::sleep(Duration::from_millis(30));
        assert!(
            rig.overlay.calls().is_empty(),
            "idle and only while recording"
        );
        rig.change(&[always()]);
        rig.calls_until(|calls| calls == [on_laptop()]);
        rig.change(&[]);
        rig.pill.exited();
        let calls = rig.calls_until(|calls| calls.contains(&OverlayCall::Hide));
        assert_eq!(calls, [on_laptop(), OverlayCall::Hide]);
    }

    #[test]
    fn a_drag_moves_the_window_with_the_cursor_until_the_button_is_released() {
        let rig = Rig::with(&[always(), movable()]);
        rig.pill.attached();
        rig.calls_until(|calls| !calls.is_empty());
        let start = ScreenPoint { x: 780, y: 944 };
        rig.overlay.move_to(start).unwrap();
        // Grabbed 150 px right of and 40 px below the corner; the first step holds the window where it is.
        rig.overlay.set_cursor(ScreenPoint { x: 930, y: 984 }, true);
        let answer = rig.pill.drag();
        rig.calls_until(|calls| count(calls, &OverlayCall::MoveTo(start)) == 2);
        rig.overlay.set_cursor(ScreenPoint { x: 330, y: 184 }, true);
        rig.calls_until(|calls| {
            calls.contains(&OverlayCall::MoveTo(ScreenPoint { x: 180, y: 144 }))
        });
        rig.overlay
            .set_cursor(ScreenPoint { x: 330, y: 184 }, false);
        let moved = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(answer)
            .unwrap();
        assert_eq!(moved, Some(ScreenPoint { x: 180, y: 144 }));

        // The stored corner is where the next show puts the pill.
        rig.change(&[
            always(),
            movable(),
            (
                keys::PILL_POSITION,
                values::pill_position(ScreenPoint { x: 180, y: 144 }),
            ),
        ]);
        rig.calls_until(|calls| {
            calls.contains(&OverlayCall::Show(OverlayPlacement::At(ScreenPoint {
                x: 180,
                y: 144,
            })))
        });
    }

    #[test]
    fn a_click_does_not_move_and_a_pill_that_is_not_movable_does_not_drag() {
        let rig = Rig::with(&[always(), movable()]);
        rig.pill.attached();
        rig.calls_until(|calls| !calls.is_empty());
        rig.overlay.set_cursor(ScreenPoint { x: 10, y: 10 }, false);
        assert_eq!(rig.drag(), None, "released before the first step");

        rig.change(&[
            always(),
            (
                keys::PILL_STYLE,
                SettingValue::Enum(StaticStr::new(PillStyle::Icon.as_str())),
            ),
        ]);
        rig.overlay.set_cursor(ScreenPoint { x: 10, y: 10 }, true);
        assert_eq!(rig.drag(), None, "not movable");
        assert!(
            !rig.overlay
                .calls()
                .iter()
                .any(|call| matches!(call, OverlayCall::MoveTo(_)))
        );
    }

    #[test]
    fn a_hidden_pill_does_not_drag_and_a_saved_corner_is_ignored_while_not_movable() {
        let saved = (
            keys::PILL_POSITION,
            values::pill_position(ScreenPoint { x: 5, y: 6 }),
        );
        let rig = Rig::with(std::slice::from_ref(&saved));
        rig.overlay.set_cursor(ScreenPoint { x: 10, y: 10 }, true);
        assert_eq!(rig.drag(), None, "not shown");
        rig.pill.emit(view(SessionStatus::Recording));
        rig.calls_until(|calls| calls == [on_laptop()]);
    }

    #[test]
    fn a_remembered_position_is_stored_announced_and_read_back() {
        let db = Db::open_in_memory().unwrap();
        let settings = SharedSettings::new(snapshot(&[]));
        let events = RecordingSink::default();
        let corner = ScreenPoint { x: -1500, y: 300 };
        remember_position(&settings, &db, &events, corner).unwrap();
        let stored = settings.current();
        assert_eq!(
            stored
                .text(&keys::PILL_POSITION)
                .and_then(values::parse_pill_position),
            Some(corner)
        );
        assert_eq!(
            registry::settings::pill_dragged_position(&stored),
            None,
            "kept, but not used while the pill is not movable"
        );
        assert_eq!(
            events.events(),
            [AppEvent::SettingsChanged(SettingsChanged {
                key: keys::PILL_POSITION,
                value: values::pill_position(corner),
            })]
        );
    }

    #[test]
    fn the_look_is_announced_only_when_it_changes() {
        let before = snapshot(&[]);
        assert_eq!(look_after_settings_change(&before, &before), None);
        assert_eq!(
            look_after_settings_change(&before, &snapshot(&[always()])),
            Some(PillLook {
                visibility: PillVisibility::Always,
                ..PillLook::default()
            })
        );
        let dragged = snapshot(&[(
            keys::PILL_POSITION,
            values::pill_position(ScreenPoint { x: 1, y: 2 }),
        )]);
        assert_eq!(
            look_after_settings_change(&before, &dragged),
            None,
            "the position is Rust's"
        );
    }
}
