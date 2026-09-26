/*!
 * SOURCE OF TRUTH KEYWORDS: HotkeyGate, pause hotkeys, resume hotkeys, capture lease, hotkeys_capture, refresh hotkeys after sleep, HotkeyStatusChanged, always-on hotkeys, rebind while paused
 * WHAT:  HotkeyGate: the one switch for Echo's always-on hotkeys (dictation, paste-last). It binds them when the
 *        session is ready (`bind`), takes them off while the user paused them or a hotkey field captures a
 *        combination (`pause` / `resume`, one reason each), re-registers them after Windows may have dropped them
 *        (`refresh`), rebinds one after its setting changes without breaking a pause (`rebind`), and reports its
 *        state (`status`, HotkeyStatusChanged).
 * WHY:   The tray, Settings, a hotkey field, power events and the session all switch the same bindings, so the state
 *        lives in one place and every caller goes through it (the session actor stays the only owner of recording
 *        state; this is only whether keys are listened to). The hotkeys come back only when no reason is left, so a
 *        capture ending never resumes hotkeys the user paused. A capture lease ends by itself after CAPTURE_LEASE, so
 *        a hotkey field that vanished without saying so (a crashed page) never leaves Echo deaf. While paused, a
 *        changed combination is still checked by the port (bound, then released) so the write is refused exactly as
 *        it would be live, and the new value is what `resume` binds. Esc (session-scoped) is not the gate's: the
 *        session guard owns it for the length of a take. Bind failures toast once (HOTKEY_UNAVAILABLE_TOAST) and the
 *        rest stay bound (05 W7). Lock order is always the settings write lock, then the gate's: a settings write
 *        calls `rebind` while it holds the former, and `bind` / `resume` take it (SharedSettings::with_writes_held)
 *        before their own, so a capture ending in the middle of a hotkey write can never bind the value that write
 *        is replacing.
 * WHERE: Built by app/bootstrap, held by the session actor (SessionConfig: `bind` on prepare, `refresh` on power
 *        events) and CommandCtx (`rebind` from settings_set / settings_reset, `pause` / `capture` from the hotkeys
 *        commands, the tray).
 */

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use parking_lot::Mutex;

use super::{hotkeys, session::HOTKEY_UNAVAILABLE_TOAST};
use crate::{
    ports::{EventSink, HotkeyService, Notifier},
    registry::hotkeys as registry_hotkeys,
    types::{
        AppEvent, HotkeyPauseReason, HotkeyScope, HotkeySpec, HotkeyStatus, HotkeyStatusChanged,
        PortResult, SettingKey, SettingsSnapshot, SharedSettings,
    },
};

/// How long a hotkey field may hold the hotkeys off without renewing (a real capture takes seconds).
pub const CAPTURE_LEASE: Duration = Duration::from_secs(60);

/// Which always-on hotkeys the gate binds (the session: the ones whose action it handles).
pub type HotkeyFilter = fn(&HotkeySpec) -> bool;

/// What the gate works through.
pub struct HotkeyGateDeps {
    pub service: Arc<dyn HotkeyService>,
    pub settings: SharedSettings,
    pub include: HotkeyFilter,
    pub notifier: Arc<dyn Notifier>,
    pub events: Arc<dyn EventSink<AppEvent>>,
    /// How long a capture holds the hotkeys off without renewing (CAPTURE_LEASE in the app).
    pub capture_lease: Duration,
}

#[derive(Default)]
struct GateState {
    /// The session is ready: without a pause, the hotkeys are bound.
    armed: bool,
    reasons: BTreeSet<HotkeyPauseReason>,
    /// Bumped by every capture start and end, so an expiry only ends the lease it was started for.
    lease: u64,
}

/// The switch for Echo's always-on hotkeys; clones share it.
#[derive(Clone)]
pub struct HotkeyGate {
    deps: Arc<HotkeyGateDeps>,
    state: Arc<Mutex<GateState>>,
}

impl HotkeyGate {
    pub fn new(deps: HotkeyGateDeps) -> Self {
        Self {
            deps: Arc::new(deps),
            state: Arc::default(),
        }
    }

    /// Whether the hotkeys are off, and why.
    pub fn status(&self) -> HotkeyStatus {
        status_of(&self.state.lock())
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: HotkeyGate::bind, session prepare binding, startup hotkeys
     * WHAT:  The session is ready: binds the always-on hotkeys now (unless paused, then on resume); false when one
     *        could not be bound. A failure is logged and toasted once, and the others stay bound.
     * WHY:   Nothing is taken from other apps before the windows exist (05 W19); the actor calls this on prepare.
     * WHERE: pipeline/session/runner.rs `prepare`.
     */
    pub fn bind(&self) -> bool {
        self.deps.settings.with_writes_held(|settings| {
            let mut state = self.state.lock();
            state.armed = true;
            !state.reasons.is_empty() || self.bind_all(settings)
        })
    }

    /// Takes the hotkeys off for `reason` (they stay off until every reason is gone).
    pub fn pause(&self, reason: HotkeyPauseReason) -> HotkeyStatus {
        let mut state = self.state.lock();
        let before = status_of(&state);
        if state.reasons.is_empty()
            && state.armed
            && let Err(error) = hotkeys::unbind_always(self.deps.service.as_ref())
        {
            tracing::warn!(detail = error.detail(), "hotkeys could not all be released");
        }
        state.reasons.insert(reason);
        self.announce(before, &state)
    }

    /// Ends the pause for `reason`; the hotkeys come back when no other reason holds them.
    pub fn resume(&self, reason: HotkeyPauseReason) -> HotkeyStatus {
        self.deps.settings.with_writes_held(|settings| {
            let mut state = self.state.lock();
            let before = status_of(&state);
            if state.reasons.remove(&reason) && state.reasons.is_empty() && state.armed {
                self.bind_all(settings);
            }
            self.announce(before, &state)
        })
    }

    /// The user's pause on (true) or off (false): the tray and Settings.
    pub fn set_paused(&self, paused: bool) -> HotkeyStatus {
        if paused {
            self.pause(HotkeyPauseReason::User)
        } else {
            self.resume(HotkeyPauseReason::User)
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: HotkeyGate::capture, capture lease, hotkey field capturing, lease expiry
     * WHAT:  A hotkey field started (true) or stopped (false) capturing: the hotkeys are off meanwhile. A start
     *        renews the lease, which ends by itself after `capture_lease` (CAPTURE_LEASE in the app).
     * WHY:   While the hook is installed Echo's own chords are swallowed or start a take, so a field could never
     *        read them; the expiry guards against a page that never says it stopped. Without an async runtime
     *        (never the case in the app) the lease simply has no expiry, which the log says.
     * WHERE: `hotkeys_capture` (ipc/commands/hotkeys.rs), from src/components/global/hotkey-input.
     */
    pub fn capture(&self, active: bool) -> HotkeyStatus {
        let lease = {
            let mut state = self.state.lock();
            state.lease = state.lease.wrapping_add(1);
            state.lease
        };
        if !active {
            return self.resume(HotkeyPauseReason::Capture);
        }
        let status = self.pause(HotkeyPauseReason::Capture);
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                let gate = self.clone();
                let lease_time = self.deps.capture_lease;
                runtime.spawn(async move {
                    tokio::time::sleep(lease_time).await;
                    gate.expire(lease);
                });
            }
            Err(_) => tracing::warn!(
                "no runtime for the capture lease; it ends only when the field says so"
            ),
        }
        status
    }

    /// Re-registers the bound hotkeys after Windows may have dropped them (sleep, unlock, explorer restart, 05 W8).
    pub fn refresh(&self) {
        let state = self.state.lock();
        if !state.armed || !state.reasons.is_empty() {
            return;
        }
        if let Err(error) = self.deps.service.refresh() {
            tracing::warn!(
                detail = error.detail(),
                "hotkeys could not be registered again"
            );
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: HotkeyGate::rebind, live hotkey rebind, rebind while paused, settings hotkey write
     * WHAT:  Binds the always-on hotkey `key` controls to its value in `settings` (None: no such hotkey, or one the
     *        gate does not bind). While paused, the combination is bound and released again, so the port still
     *        refuses a bad one, and `resume` binds it later.
     * WHY:   A settings write must fail on a refused combination whether or not the hotkeys are paused (05 W7), and
     *        a write while paused must not quietly turn one hotkey back on.
     * WHERE: ipc/commands/settings.rs (the write and the put-back after a failed store).
     */
    pub fn rebind(&self, key: &SettingKey, settings: &SettingsSnapshot) -> Option<PortResult<()>> {
        let state = self.state.lock();
        let service = self.deps.service.as_ref();
        let bound = hotkeys::rebind_setting(service, key, settings, self.deps.include)?;
        if bound.is_ok() && !state.reasons.is_empty() {
            let released = registry_hotkeys::for_setting(key)
                .filter(|spec| spec.scope == HotkeyScope::Always)
                .map_or(Ok(()), |spec| service.unregister(&spec.id));
            return Some(released);
        }
        Some(bound)
    }

    fn expire(&self, lease: u64) {
        let current = self.state.lock().lease == lease;
        if current {
            tracing::info!("a hotkey capture lease ran out; hotkeys are back");
            self.resume(HotkeyPauseReason::Capture);
        }
    }

    /// Binds every always-on hotkey the filter includes at `settings`; toasts once when one fails. True when all
    /// bound.
    fn bind_all(&self, settings: &SettingsSnapshot) -> bool {
        let failures =
            hotkeys::bind_always_where(self.deps.service.as_ref(), settings, self.deps.include);
        if !failures.is_empty()
            && let Err(error) = self.deps.notifier.toast(&HOTKEY_UNAVAILABLE_TOAST)
        {
            tracing::warn!(detail = error.detail(), "a toast could not be shown");
        }
        failures.is_empty()
    }

    fn announce(&self, before: HotkeyStatus, state: &GateState) -> HotkeyStatus {
        let after = status_of(state);
        if after != before {
            tracing::info!(
                paused = after.paused,
                capturing = after.capturing,
                "hotkeys switched"
            );
            self.deps.events.emit(HotkeyStatusChanged(after).into());
        }
        after
    }
}

fn status_of(state: &GateState) -> HotkeyStatus {
    HotkeyStatus {
        paused: state.reasons.contains(&HotkeyPauseReason::User),
        capturing: state.reasons.contains(&HotkeyPauseReason::Capture),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::fakes::{FakeHotkeyService, FakeNotifier, RecordingSink},
        registry::{
            hotkeys::{PASTE_LAST, RECORD},
            settings::{self, keys},
        },
        types::{AppError, HotkeyIssue, SettingValue, Shortcut, StaticStr},
    };

    /// A lease short enough for a test to outwait.
    const TEST_LEASE: Duration = Duration::from_millis(200);

    struct Rig {
        gate: HotkeyGate,
        service: Arc<FakeHotkeyService>,
        notifier: Arc<FakeNotifier>,
        events: Arc<RecordingSink<AppEvent>>,
        settings: SharedSettings,
    }

    fn rig() -> Rig {
        let service = Arc::new(FakeHotkeyService::default());
        let notifier = Arc::new(FakeNotifier::default());
        let events = Arc::new(RecordingSink::default());
        let settings = SharedSettings::new(settings::defaults());
        let gate = HotkeyGate::new(HotkeyGateDeps {
            service: Arc::clone(&service) as _,
            settings: settings.clone(),
            include: |_| true,
            notifier: Arc::clone(&notifier) as _,
            events: Arc::clone(&events) as _,
            capture_lease: TEST_LEASE,
        });
        Rig {
            gate,
            service,
            notifier,
            events,
            settings,
        }
    }

    fn statuses(events: &RecordingSink<AppEvent>) -> Vec<HotkeyStatus> {
        events
            .events()
            .into_iter()
            .filter_map(|event| match event {
                AppEvent::HotkeyStatusChanged(HotkeyStatusChanged(status)) => Some(status),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn nothing_binds_before_the_session_is_ready_and_a_pause_holds_until_every_reason_ends() {
        let rig = rig();
        rig.gate.pause(HotkeyPauseReason::User);
        assert_eq!(rig.service.binding(&RECORD), None);
        assert!(rig.gate.bind(), "a paused gate has nothing to fail");
        assert_eq!(rig.service.binding(&RECORD), None, "paused before prepare");

        rig.gate.capture(true);
        rig.gate.set_paused(false);
        assert_eq!(
            rig.service.binding(&RECORD),
            None,
            "the capture still holds them"
        );
        let status = rig.gate.capture(false);
        assert_eq!(status, HotkeyStatus::default());
        assert!(rig.service.binding(&RECORD).is_some());
        assert!(rig.service.binding(&PASTE_LAST).is_some());
        assert_eq!(
            statuses(&rig.events),
            [
                HotkeyStatus {
                    paused: true,
                    capturing: false
                },
                HotkeyStatus {
                    paused: true,
                    capturing: true
                },
                HotkeyStatus {
                    paused: false,
                    capturing: true
                },
                HotkeyStatus::default(),
            ]
        );
    }

    #[test]
    fn pausing_releases_and_resuming_binds_the_current_settings() {
        let rig = rig();
        assert!(rig.gate.bind());
        rig.gate.set_paused(true);
        assert_eq!(rig.service.binding(&RECORD), None);
        rig.settings.replace(settings::resolve([(
            keys::RECORD_HOTKEY,
            SettingValue::Hotkey(StaticStr::new("Ctrl+Shift+F9")),
        )]));
        rig.gate.set_paused(false);
        assert_eq!(
            rig.service.binding(&RECORD),
            Some(Shortcut::from_static("Ctrl+Shift+F9"))
        );
        assert!(rig.gate.status() == HotkeyStatus::default());
    }

    #[test]
    fn a_rebind_while_paused_is_checked_but_stays_off() {
        let rig = rig();
        assert!(rig.gate.bind());
        rig.gate.set_paused(true);
        let moved = settings::resolve([(
            keys::RECORD_HOTKEY,
            SettingValue::Hotkey(StaticStr::new("Ctrl+Alt+F8")),
        )]);
        assert!(matches!(
            rig.gate.rebind(&keys::RECORD_HOTKEY, &moved),
            Some(Ok(()))
        ));
        assert_eq!(rig.service.binding(&RECORD), None, "still paused");

        rig.service.occupy(Shortcut::from_static("Ctrl+Alt+F7"));
        let taken = settings::resolve([(
            keys::RECORD_HOTKEY,
            SettingValue::Hotkey(StaticStr::new("Ctrl+Alt+F7")),
        )]);
        let refused = rig
            .gate
            .rebind(&keys::RECORD_HOTKEY, &taken)
            .map(|result| result.map_err(|error| error.into_app_error()));
        assert_eq!(
            refused,
            Some(Err(AppError::Hotkey {
                reason: HotkeyIssue::Conflict
            }))
        );
        assert!(rig.gate.rebind(&keys::THEME, &taken).is_none());
    }

    #[test]
    fn refresh_touches_only_live_bindings_and_a_failed_bind_toasts() {
        let rig = rig();
        rig.gate.refresh();
        assert_eq!(rig.service.refreshes(), 0, "not bound yet");
        assert!(rig.gate.bind());
        rig.gate.refresh();
        assert_eq!(rig.service.refreshes(), 1);
        rig.gate.set_paused(true);
        rig.gate.refresh();
        assert_eq!(
            rig.service.refreshes(),
            1,
            "paused hotkeys are not re-registered"
        );

        let other = self::rig();
        other.service.occupy(Shortcut::from_static(
            crate::registry::hotkeys::RECORD_DEFAULT,
        ));
        assert!(!other.gate.bind());
        assert_eq!(other.notifier.toasts(), [HOTKEY_UNAVAILABLE_TOAST]);
    }

    #[test]
    fn a_capture_lease_runs_out_by_itself() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let rig = rig();
        assert!(rig.gate.bind());
        runtime.block_on(async {
            rig.gate.capture(true);
            assert!(rig.gate.status().capturing);
            tokio::time::sleep(TEST_LEASE / 2).await;
            // A renewed lease outlives the first one's expiry.
            rig.gate.capture(true);
            tokio::time::sleep(TEST_LEASE * 3 / 4).await;
            assert!(
                rig.gate.status().capturing,
                "the first lease's expiry is stale"
            );
            tokio::time::sleep(TEST_LEASE).await;
        });
        assert!(!rig.gate.status().capturing);
        assert!(rig.service.binding(&RECORD).is_some());
        assert!(CAPTURE_LEASE > TEST_LEASE);
    }
}
