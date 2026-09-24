/*!
 * SOURCE OF TRUTH KEYWORDS: PillPresenter, PillTiming, pill window show hide, pill exit animation, pill_exited, pill hit areas, click-through polling, pill placement
 * WHAT:  PillPresenter: follows SessionStateChanged and keeps the pill window in step with the session. It shows the
 *        pill (on the monitor of the window the user is typing in) the moment a take leaves Idle, hides it once the
 *        page says its exit animation finished (or after a fallback delay), and, while the pill shows buttons,
 *        polls the pointer so clicks pass through everywhere except on those buttons.
 * WHY:   The session actor owns recording state and knows nothing about windows; the presenter observes the same
 *        event stream the UI gets (through FanOut), so the window can never disagree with the view (02 §5). Showing
 *        waits for nothing: the window is pre-created and shown without activation on the first non-Idle view, so
 *        "hotkey → pill visible" stays under 50 ms (02 §6.2). Hiding waits for the page's exit animation
 *        (`pill_exited`), because hiding first would cut it off; if the page never answers (it crashed, it is
 *        still loading) EXIT_FALLBACK hides it anyway. A take that starts while the pill is leaving cancels the hide
 *        and moves the pill to the new take's monitor. A click-through window gets no pointer events, so only Rust
 *        can tell that the pointer reached a button: the pointer is polled only while the pill is shown with
 *        buttons, and never otherwise. All window work runs on the presenter's own thread, so `emit` (called by the
 *        session actor) only queues a message and never waits for the window manager. Failures only cost looks and
 *        are logged; the take itself never depends on the pill.
 * WHERE: Spawned by app/bootstrap and wired into the event FanOut; `set_hit_areas` and `exited` are called by
 *        ipc/commands/pill.rs (`pill_set_hit_areas`, `pill_exited`) from src/pill. Works through the OverlayWindow and
 *        ForegroundApp ports.
 */

use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use crate::{
    ports::{EventSink, ForegroundApp, OverlayWindow},
    types::{
        AppError, AppEvent, OverlayRect, PortError, PortResult, ScreenRect, SessionStateChanged,
        SessionStatus,
    },
};

/**
 * SOURCE OF TRUTH KEYWORDS: PillTiming, pointer poll interval, exit fallback
 * WHAT:  How often the pointer is checked against the pill's buttons, and how long the pill may stay up after the
 *        session went Idle when the page never reports its exit animation.
 * WHY:   30 Hz makes a button feel instantly clickable while costing nothing measurable (a cursor read); the
 *        fallback outlasts the slowest exit spring (pillExit settles in well under a second, 04 §3.7).
 * WHERE: PillPresenter::spawn (DEFAULT in the app, shorter values in tests).
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PillTiming {
    pub pointer_poll: Duration,
    pub exit_fallback: Duration,
}

impl PillTiming {
    pub const DEFAULT: Self = Self {
        pointer_poll: Duration::from_millis(33),
        exit_fallback: Duration::from_millis(1_500),
    };
}

enum PillMessage {
    Status(SessionStatus),
    HitAreas(Vec<OverlayRect>),
    Exited,
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
        timing: PillTiming,
    ) -> PortResult<Self> {
        let (inbox, messages) = mpsc::channel();
        let worker = Worker {
            overlay,
            foreground,
            timing,
            shown: false,
            hide_at: None,
            areas: Vec::new(),
            taking_clicks: false,
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

    /// The page's buttons are now at `areas` (CSS pixels of the pill page); empty when it shows none.
    pub fn set_hit_areas(&self, areas: Vec<OverlayRect>) {
        self.send(PillMessage::HitAreas(areas));
    }

    /// The page finished its exit animation: the window can go.
    pub fn exited(&self) {
        self.send(PillMessage::Exited);
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

/// The presenter thread's state.
struct Worker {
    overlay: Arc<dyn OverlayWindow>,
    foreground: Arc<dyn ForegroundApp>,
    timing: PillTiming,
    /// The window is up (possibly playing its exit animation).
    shown: bool,
    /// The session is Idle: hide at this instant unless the page says it finished first.
    hide_at: Option<Instant>,
    /// The page's buttons.
    areas: Vec<OverlayRect>,
    /// Click-through is off because the pointer is on a button.
    taking_clicks: bool,
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

    /// How long the thread may sleep: until the next pointer check or the fallback hide; None waits for a message.
    fn next_wake(&self, now: Instant) -> Option<Duration> {
        let poll = (self.shown && !self.areas.is_empty()).then_some(self.timing.pointer_poll);
        let hide = self.hide_at.map(|at| at.saturating_duration_since(now));
        match (poll, hide) {
            (Some(poll), Some(hide)) => Some(poll.min(hide)),
            (poll, hide) => poll.or(hide),
        }
    }

    fn handle(&mut self, message: PillMessage) {
        match message {
            PillMessage::Status(SessionStatus::Idle) => {
                if self.shown && self.hide_at.is_none() {
                    self.hide_at = Some(Instant::now() + self.timing.exit_fallback);
                }
            }
            PillMessage::Status(_) => {
                // A take starting (or one starting while the last one's pill is leaving) places the pill again.
                let appearing = !self.shown || self.hide_at.is_some();
                self.hide_at = None;
                if appearing {
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
        }
    }

    fn tick(&mut self, now: Instant) {
        if self.hide_at.is_some_and(|at| at <= now) {
            self.hide();
        }
        if self.shown && !self.areas.is_empty() {
            let over = self
                .overlay
                .pointer_over(&self.areas)
                .unwrap_or_else(|error| {
                    tracing::debug!(detail = error.detail(), "the pointer could not be read");
                    false
                });
            self.take_clicks(over);
        }
    }

    /// Shows the pill on the monitor of the window that has focus now (the take's target).
    fn show(&mut self) {
        let work_area = self.target_work_area();
        match self.overlay.show(work_area) {
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
        self.hide_at = None;
        self.areas.clear();
        self.take_clicks(false);
        if let Err(error) = self.overlay.hide() {
            tracing::warn!(detail = error.detail(), "the pill could not be hidden");
        }
        self.shown = false;
    }

    /// Switches click-through off while the pointer is on a button, back on otherwise.
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
        ports::fakes::{FakeForegroundApp, FakeOverlayWindow, OverlayCall},
        types::SessionView,
    };

    const TIMING: PillTiming = PillTiming {
        pointer_poll: Duration::from_millis(5),
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

    struct Rig {
        overlay: Arc<FakeOverlayWindow>,
        foreground: Arc<FakeForegroundApp>,
        pill: PillPresenter,
    }

    impl Rig {
        fn new() -> Self {
            let overlay = Arc::new(FakeOverlayWindow::default());
            let foreground = Arc::new(FakeForegroundApp::default());
            let mut target = FakeForegroundApp::target("notepad.exe", false);
            target.work_area = Some(laptop());
            foreground.set(Some(target));
            let pill = PillPresenter::spawn(overlay.clone(), foreground.clone(), TIMING).unwrap();
            Self {
                overlay,
                foreground,
                pill,
            }
        }

        fn calls_until(&self, done: impl Fn(&[OverlayCall]) -> bool) -> Vec<OverlayCall> {
            self.overlay.wait_for(WAIT, done)
        }
    }

    fn count(calls: &[OverlayCall], call: &OverlayCall) -> usize {
        calls.iter().filter(|seen| *seen == call).count()
    }

    #[test]
    fn a_take_shows_the_pill_once_on_the_targets_monitor_and_the_exit_hides_it() {
        let rig = Rig::new();
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
        assert_eq!(
            calls,
            [OverlayCall::Show(Some(laptop())), OverlayCall::Hide]
        );
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
                OverlayCall::Show(Some(laptop())),
                OverlayCall::Show(Some(external))
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
        assert_eq!(calls, [OverlayCall::Show(None)]);
    }
}
