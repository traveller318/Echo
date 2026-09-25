/*!
 * SOURCE OF TRUTH KEYWORDS: CommandCtx, CommandDeps, command context, managed state, handler dependencies, SharedSettings, AsrWorker, SessionEngines, RetryDeps, Delivery, SessionHandle, PillPresenter, AppPaths, Db, event sink
 * WHAT:  CommandCtx: everything a command handler and the factory pipeline may use, managed once by Tauri and
 *        passed to every handler as `&CommandCtx`; CommandDeps: the named parts it is built from.
 * WHY:   Handlers take their dependencies from one place instead of Tauri state lookups, so they stay plain async
 *        fns that tests call with a context built from port fakes and an in-memory database. It holds only ports
 *        (`Arc<dyn …>`, never an adapter, 02 §3.2), the services' `Db` handle, the resolved AppPaths (paths are
 *        resolved only in app/, 05 W23), shared handles (settings, the ASR worker that owns the speech engine, the
 *        delivery that owns the paste rules, the session actor's handle, the take engines the actor shares with
 *        retry) and the factory's own reentrancy locks; the
 *        registry needs no handle because it is compiled-in `const` data. Events leave through the
 *        `EventSink<AppEvent>` port, so no handler imports Tauri. Built from a named-field struct so a new
 *        dependency is one field here plus one line where it is wired. The session actor is reached through its
 *        SessionHandle (the pill's stop, the current view), never through a copy of its state; the pill window
 *        through its PillPresenter (button areas, exit animation) and the main window through the MainWindow port.
 * WHERE: Built by app/bootstrap and managed on the Tauri app; read by ipc/factory.rs (preflight, reentrancy)
 *        and every handler in ipc/commands. Tests build it with `testing::harness`.
 */

use std::sync::Arc;

use super::reentrancy::ReentrancyLocks;
use crate::{
    pipeline::{
        asr::AsrWorker,
        delivery::Delivery,
        pill::PillPresenter,
        retry::RetryDeps,
        session::{SessionEngines, SessionHandle},
    },
    ports::{
        AudioCapture, EventSink, ForegroundApp, HotkeyService, MainWindow, Notifier,
        PrivacyConsent, SystemAppearance, SystemLauncher, WorkerScheduler,
    },
    services::Db,
    types::{AppEvent, AppPaths, SettingsSnapshot, SharedSettings},
};

/// The parts a CommandCtx is built from.
pub struct CommandDeps {
    /// The one live settings snapshot (resolved from the database at startup).
    pub settings: SharedSettings,
    /// Operating-system privacy consent, for the Microphone permission check.
    pub consent: Arc<dyn PrivacyConsent>,
    /// Operating-system appearance (transparency switch, Mica support), for the appearance view.
    pub appearance: Arc<dyn SystemAppearance>,
    /// Hands folders and settings pages to the operating system's own UI.
    pub launcher: Arc<dyn SystemLauncher>,
    /// The microphone: device list and the microphone check (the session actor shares the same instance).
    pub audio: Arc<dyn AudioCapture>,
    /// Thread priorities for the pipeline workers a command starts (the microphone check's capture worker).
    pub scheduler: Arc<dyn WorkerScheduler>,
    /// The thread that owns the speech engine: startup load, engine switch, takes (the session actor shares it).
    pub asr: AsrWorker,
    /// System-wide hotkeys: bound by the session actor, rebound when a hotkey setting changes.
    pub hotkeys: Arc<dyn HotkeyService>,
    /// The focused window: the take's paste target and paste-last's.
    pub foreground: Arc<dyn ForegroundApp>,
    /// Native toasts for moments without a pill (recovered takes, device lost).
    pub notifier: Arc<dyn Notifier>,
    /// Clipboard + paste rules for delivered text (takes, history copy, paste-last).
    pub delivery: Delivery,
    /// The session actor (sole owner of recording state): the current view and the pill's inputs.
    pub session: SessionHandle,
    /// The detector builder and polish chain the session actor uses (shared with it), for retry.
    pub engines: SessionEngines,
    /// The pill window's presenter: where the page's buttons are, when its exit animation ended.
    pub pill: PillPresenter,
    /// Echo's main window, brought forward by surfaces outside it (the pill, later the tray).
    pub main_window: Arc<dyn MainWindow>,
    /// Every data and resource location, resolved once by app/bootstrap.
    pub paths: AppPaths,
    /// The database every service call goes through.
    pub db: Db,
    /// Where commands send Rust → UI events.
    pub events: Arc<dyn EventSink<AppEvent>>,
}

/// What command handlers and the factory may use.
pub struct CommandCtx {
    settings: SharedSettings,
    consent: Arc<dyn PrivacyConsent>,
    appearance: Arc<dyn SystemAppearance>,
    launcher: Arc<dyn SystemLauncher>,
    audio: Arc<dyn AudioCapture>,
    scheduler: Arc<dyn WorkerScheduler>,
    asr: AsrWorker,
    hotkeys: Arc<dyn HotkeyService>,
    foreground: Arc<dyn ForegroundApp>,
    notifier: Arc<dyn Notifier>,
    delivery: Delivery,
    session: SessionHandle,
    engines: SessionEngines,
    pill: PillPresenter,
    main_window: Arc<dyn MainWindow>,
    paths: AppPaths,
    db: Db,
    events: Arc<dyn EventSink<AppEvent>>,
    locks: ReentrancyLocks,
}

impl CommandCtx {
    pub fn new(deps: CommandDeps) -> Self {
        let CommandDeps {
            settings,
            consent,
            appearance,
            launcher,
            audio,
            scheduler,
            asr,
            hotkeys,
            foreground,
            notifier,
            delivery,
            session,
            engines,
            pill,
            main_window,
            paths,
            db,
            events,
        } = deps;
        Self {
            settings,
            consent,
            appearance,
            launcher,
            audio,
            scheduler,
            asr,
            hotkeys,
            foreground,
            notifier,
            delivery,
            session,
            engines,
            pill,
            main_window,
            paths,
            db,
            events,
            locks: ReentrancyLocks::default(),
        }
    }

    /// The settings in effect now.
    pub fn settings(&self) -> Arc<SettingsSnapshot> {
        self.settings.current()
    }

    /// The live settings handle, for handlers that replace the snapshot after a write.
    pub fn shared_settings(&self) -> &SharedSettings {
        &self.settings
    }

    /// Operating-system privacy consent (the Microphone permission check).
    pub fn consent(&self) -> &dyn PrivacyConsent {
        self.consent.as_ref()
    }

    /// Operating-system appearance (the appearance view).
    pub fn appearance(&self) -> &dyn SystemAppearance {
        self.appearance.as_ref()
    }

    /// The operating system's folder and settings-page opener.
    pub fn launcher(&self) -> &dyn SystemLauncher {
        self.launcher.as_ref()
    }

    /// The microphone.
    pub fn audio(&self) -> &dyn AudioCapture {
        self.audio.as_ref()
    }

    /// Thread priorities for pipeline workers a command starts.
    pub fn scheduler(&self) -> Arc<dyn WorkerScheduler> {
        Arc::clone(&self.scheduler)
    }

    /// The ASR worker (speech engine owner).
    pub fn asr(&self) -> &AsrWorker {
        &self.asr
    }

    /// System-wide hotkeys.
    pub fn hotkeys(&self) -> Arc<dyn HotkeyService> {
        Arc::clone(&self.hotkeys)
    }

    /// The focused window.
    pub fn foreground(&self) -> &dyn ForegroundApp {
        self.foreground.as_ref()
    }

    /// Native toasts.
    pub fn notifier(&self) -> &dyn Notifier {
        self.notifier.as_ref()
    }

    /// Clipboard + paste delivery.
    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }

    /// The session actor's handle.
    pub fn session(&self) -> &SessionHandle {
        &self.session
    }

    /// What a retry works through: the same ASR worker, engines, paths, database and events as a live take.
    pub fn retry_deps(&self) -> RetryDeps {
        RetryDeps {
            asr: self.asr.clone(),
            engines: self.engines.clone(),
            paths: self.paths.clone(),
            db: self.db.clone(),
            events: Arc::clone(&self.events),
        }
    }

    /// The event sink, for pipeline calls that announce their own changes.
    pub fn events(&self) -> &dyn EventSink<AppEvent> {
        self.events.as_ref()
    }

    /// The pill window's presenter.
    pub fn pill(&self) -> &PillPresenter {
        &self.pill
    }

    /// Echo's main window.
    pub fn main_window(&self) -> &dyn MainWindow {
        self.main_window.as_ref()
    }

    /// Every data and resource location.
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    /// The database handle services take.
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Sends a Rust → UI event; never blocks.
    pub fn emit(&self, event: impl Into<AppEvent>) {
        self.events.emit(event.into());
    }

    /// The factory's keyed reentrancy locks.
    pub(super) fn locks(&self) -> &ReentrancyLocks {
        &self.locks
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: command test harness, test CommandCtx, in-memory database context, recorded events
 * WHAT:  `harness`: a CommandCtx over given settings and consent, a Mica-capable appearance fake, a recording
 *        launcher, a 48 kHz stereo capture fake, a recording thread scheduler, an ASR worker whose engines are
 *        English FakeAsrEngines (nothing loaded until a test asks), a hotkey fake, a focused Notepad target, a
 *        recording notifier, a delivery over a clipboard fake and a pasting inserter fake, a session handle whose
 *        actor (fake detector, registry polishers) is built but not running, a pill presenter over an overlay fake,
 *        a main-window fake, AppPaths under the system temp folder
 *        (never touched: services take the in-memory database), a fresh in-memory database with the real migrations
 *        and a RecordingSink for events, plus handles to the sink and the fakes; `ctx()` is the all-defaults one.
 * WHY:   Factory, command and app tests all need the same context without a Tauri app or a disk; keeping the
 *        builder here means a new CommandDeps field is added to tests in one place.
 * WHERE: Tests in ipc/factory.rs, ipc/commands and app/bindings.rs.
 */
#[cfg(test)]
pub mod testing {
    use std::sync::Arc;

    use super::{CommandCtx, CommandDeps};
    use crate::{
        pipeline::{
            asr::{AsrWorker, AsrWorkerConfig},
            delivery::{Delivery, DeliveryPorts},
            pill::{PillPresenter, PillTiming},
            session::{SessionActor, SessionConfig, SessionEngines, SessionHandle},
        },
        ports::{
            AsrEngine,
            fakes::{
                FakeAsrEngine, FakeAudioCapture, FakeClipboard, FakeForegroundApp,
                FakeHotkeyService, FakeMainWindow, FakeNotifier, FakeOverlayWindow,
                FakePrivacyConsent, FakeSystemAppearance, FakeSystemLauncher, FakeTextInserter,
                FakeVoiceActivity, FakeWorkerScheduler, RecordingSink,
            },
        },
        registry::{self, engines::BuildCtx},
        services::Db,
        types::{AppEvent, AppPaths, CaptureFormat, EngineId, SettingsSnapshot, SharedSettings},
    };

    /// The format the harness microphone delivers.
    pub const HARNESS_AUDIO_FORMAT: CaptureFormat = CaptureFormat {
        sample_rate: 48_000,
        channels: 2,
    };

    /// A test context, the sink its events land in and its fakes (appearance: Mica, transparency on).
    pub struct Harness {
        pub ctx: CommandCtx,
        /// The session actor over the same fakes, not running: a test spawns `run` on its own runtime, or drops
        /// it (the session commands then answer `Internal`).
        pub session_actor: SessionActor,
        pub events: Arc<RecordingSink<AppEvent>>,
        pub appearance: Arc<FakeSystemAppearance>,
        pub launcher: Arc<FakeSystemLauncher>,
        pub audio: Arc<FakeAudioCapture>,
        pub scheduler: Arc<FakeWorkerScheduler>,
        pub hotkeys: Arc<FakeHotkeyService>,
        pub foreground: Arc<FakeForegroundApp>,
        pub notifier: Arc<FakeNotifier>,
        pub clipboard: Arc<FakeClipboard>,
        pub inserter: Arc<FakeTextInserter>,
        pub overlay: Arc<FakeOverlayWindow>,
        pub main_window: Arc<FakeMainWindow>,
    }

    pub fn harness(settings: SettingsSnapshot, consent: FakePrivacyConsent) -> Harness {
        let events = Arc::new(RecordingSink::default());
        let appearance = Arc::new(FakeSystemAppearance::mica());
        let launcher = Arc::new(FakeSystemLauncher::default());
        let audio = Arc::new(FakeAudioCapture::new(HARNESS_AUDIO_FORMAT));
        let scheduler = Arc::new(FakeWorkerScheduler::default());
        let hotkeys = Arc::new(FakeHotkeyService::default());
        let foreground = Arc::new(FakeForegroundApp::focused(FakeForegroundApp::target(
            "notepad.exe",
            false,
        )));
        let notifier = Arc::new(FakeNotifier::default());
        let clipboard = Arc::new(FakeClipboard::default());
        let inserter = Arc::new(FakeTextInserter::default());
        let overlay = Arc::new(FakeOverlayWindow::default());
        let main_window = Arc::new(FakeMainWindow::default());
        let pill = PillPresenter::spawn(
            Arc::clone(&overlay) as _,
            Arc::clone(&foreground) as _,
            PillTiming::DEFAULT,
        )
        .unwrap();
        let delivery = Delivery::new(DeliveryPorts {
            clipboard: Arc::clone(&clipboard) as _,
            inserter: Arc::clone(&inserter) as _,
            notifier: Arc::clone(&notifier) as _,
        });
        let root = std::env::temp_dir().join("echo-harness");
        let paths = AppPaths::new(root.join("data"), root.join("resources"));
        let asr = AsrWorker::spawn(AsrWorkerConfig {
            build: Arc::new(|_: &EngineId| {
                Ok(Arc::new(FakeAsrEngine::english()) as Arc<dyn AsrEngine>)
            }),
            // Its own scheduler: the worker thread's request must not race the harness scheduler's assertions.
            scheduler: Arc::new(FakeWorkerScheduler::default()),
            readiness: None,
        })
        .unwrap();
        let settings = SharedSettings::new(settings);
        let db = Db::open_in_memory().unwrap();
        let (session, inbox) = SessionHandle::new();
        let polish_ctx = BuildCtx {
            paths: paths.clone(),
        };
        let engines = SessionEngines::new(
            Arc::new(|| Ok(Box::new(FakeVoiceActivity::new(32)) as _)),
            Arc::new(move |id: &EngineId| registry::engines::build_polisher(id, &polish_ctx)),
        );
        let session_actor = SessionActor::new(
            SessionConfig {
                settings: settings.clone(),
                audio: Arc::clone(&audio) as _,
                // Its own scheduler, for the same reason as the ASR worker's.
                scheduler: Arc::new(FakeWorkerScheduler::default()),
                asr: asr.clone(),
                hotkeys: Arc::clone(&hotkeys) as _,
                foreground: Arc::clone(&foreground) as _,
                notifier: Arc::clone(&notifier) as _,
                delivery: delivery.clone(),
                paths: paths.clone(),
                db: db.clone(),
                events: Arc::clone(&events) as _,
                engines: engines.clone(),
            },
            inbox,
        );
        let ctx = CommandCtx::new(CommandDeps {
            settings,
            consent: Arc::new(consent),
            appearance: Arc::clone(&appearance) as _,
            launcher: Arc::clone(&launcher) as _,
            audio: Arc::clone(&audio) as _,
            scheduler: Arc::clone(&scheduler) as _,
            asr,
            hotkeys: Arc::clone(&hotkeys) as _,
            foreground: Arc::clone(&foreground) as _,
            notifier: Arc::clone(&notifier) as _,
            delivery,
            session,
            engines,
            pill,
            main_window: Arc::clone(&main_window) as _,
            paths,
            db,
            events: Arc::clone(&events) as _,
        });
        Harness {
            ctx,
            session_actor,
            events,
            appearance,
            launcher,
            audio,
            scheduler,
            hotkeys,
            foreground,
            notifier,
            clipboard,
            inserter,
            overlay,
            main_window,
        }
    }

    /// Registry defaults, granted consent, an empty database.
    pub fn ctx() -> CommandCtx {
        harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        )
        .ctx
    }
}
