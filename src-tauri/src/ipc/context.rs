/*!
 * SOURCE OF TRUTH KEYWORDS: CommandCtx, CommandDeps, command context, managed state, handler dependencies, SharedSettings, AudioCapture, WorkerScheduler, SystemLauncher, AppPaths, Db, event sink
 * WHAT:  CommandCtx: everything a command handler and the factory pipeline may use, managed once by Tauri and
 *        passed to every handler as `&CommandCtx`; CommandDeps: the named parts it is built from.
 * WHY:   Handlers take their dependencies from one place instead of Tauri state lookups, so they stay plain async
 *        fns that tests call with a context built from port fakes and an in-memory database. It holds only ports
 *        (`Arc<dyn …>`, never an adapter, 02 §3.2), the services' `Db` handle, the resolved AppPaths (paths are
 *        resolved only in app/, 05 W23), shared handles and the factory's own reentrancy locks; the registry needs no handle because it is compiled-in `const` data. Events leave
 *        through the `EventSink<AppEvent>` port, so no handler imports Tauri. Built from a named-field struct so a
 *        new dependency (the session actor's inbox, step 14) is one field here plus one line where it is wired.
 * WHERE: Built by app/bootstrap and managed on the Tauri app; read by ipc/factory.rs (preflight, reentrancy)
 *        and every handler in ipc/commands. Tests build it with `testing::harness`.
 */

use std::sync::Arc;

use super::reentrancy::ReentrancyLocks;
use crate::{
    ports::{
        AudioCapture, EventSink, PrivacyConsent, SystemAppearance, SystemLauncher, WorkerScheduler,
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
 *        launcher, a 48 kHz stereo capture fake, a recording thread scheduler, AppPaths under the system temp
 *        folder (never touched: services take the in-memory database), a fresh in-memory database with the real
 *        migrations and a RecordingSink for events, plus handles to the sink and the fakes; `ctx()` is the
 *        all-defaults one.
 * WHY:   Factory, command and app tests all need the same context without a Tauri app or a disk; keeping the
 *        builder here means a new CommandDeps field is added to tests in one place.
 * WHERE: Tests in ipc/factory.rs, ipc/commands and app/bindings.rs.
 */
#[cfg(test)]
pub mod testing {
    use std::sync::Arc;

    use super::{CommandCtx, CommandDeps};
    use crate::{
        ports::fakes::{
            FakeAudioCapture, FakePrivacyConsent, FakeSystemAppearance, FakeSystemLauncher,
            FakeWorkerScheduler, RecordingSink,
        },
        registry,
        services::Db,
        types::{AppEvent, AppPaths, CaptureFormat, SettingsSnapshot, SharedSettings},
    };

    /// The format the harness microphone delivers.
    pub const HARNESS_AUDIO_FORMAT: CaptureFormat = CaptureFormat {
        sample_rate: 48_000,
        channels: 2,
    };

    /// A test context, the sink its events land in and its fakes (appearance: Mica, transparency on).
    pub struct Harness {
        pub ctx: CommandCtx,
        pub events: Arc<RecordingSink<AppEvent>>,
        pub appearance: Arc<FakeSystemAppearance>,
        pub launcher: Arc<FakeSystemLauncher>,
        pub audio: Arc<FakeAudioCapture>,
        pub scheduler: Arc<FakeWorkerScheduler>,
    }

    pub fn harness(settings: SettingsSnapshot, consent: FakePrivacyConsent) -> Harness {
        let events = Arc::new(RecordingSink::default());
        let appearance = Arc::new(FakeSystemAppearance::mica());
        let launcher = Arc::new(FakeSystemLauncher::default());
        let audio = Arc::new(FakeAudioCapture::new(HARNESS_AUDIO_FORMAT));
        let scheduler = Arc::new(FakeWorkerScheduler::default());
        let root = std::env::temp_dir().join("echo-harness");
        let ctx = CommandCtx::new(CommandDeps {
            settings: SharedSettings::new(settings),
            consent: Arc::new(consent),
            appearance: Arc::clone(&appearance) as _,
            launcher: Arc::clone(&launcher) as _,
            audio: Arc::clone(&audio) as _,
            scheduler: Arc::clone(&scheduler) as _,
            paths: AppPaths::new(root.join("data"), root.join("resources")),
            db: Db::open_in_memory().unwrap(),
            events: Arc::clone(&events) as _,
        });
        Harness {
            ctx,
            events,
            appearance,
            launcher,
            audio,
            scheduler,
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
