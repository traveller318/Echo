/*!
 * SOURCE OF TRUTH KEYWORDS: CommandCtx, CommandDeps, command context, managed state, handler dependencies, SharedSettings, SystemAppearance, Db, event sink, emit, reentrancy locks
 * WHAT:  CommandCtx: everything a command handler and the factory pipeline may use, managed once by Tauri and
 *        passed to every handler as `&CommandCtx`; CommandDeps: the named parts it is built from.
 * WHY:   Handlers take their dependencies from one place instead of Tauri state lookups, so they stay plain async
 *        fns that tests call with a context built from port fakes and an in-memory database. It holds only ports
 *        (`Arc<dyn …>`, never an adapter, 02 §3.2), the services' `Db` handle, shared handles and the factory's
 *        own reentrancy locks; the registry needs no handle because it is compiled-in `const` data. Events leave
 *        through the `EventSink<AppEvent>` port, so no handler imports Tauri. Built from a named-field struct so a
 *        new dependency (the session actor's inbox, step 14) is one field here plus one line where it is wired.
 * WHERE: Built by app/bootstrap and managed on the Tauri app; read by ipc/factory.rs (preflight, reentrancy)
 *        and every handler in ipc/commands. Tests build it with `testing::harness`.
 */

use std::sync::Arc;

use super::reentrancy::ReentrancyLocks;
use crate::{
    ports::{EventSink, PrivacyConsent, SystemAppearance},
    services::Db,
    types::{AppEvent, SettingsSnapshot, SharedSettings},
};

/// The parts a CommandCtx is built from.
pub struct CommandDeps {
    /// The one live settings snapshot (resolved from the database at startup).
    pub settings: SharedSettings,
    /// Operating-system privacy consent, for the Microphone permission check.
    pub consent: Arc<dyn PrivacyConsent>,
    /// Operating-system appearance (transparency switch, Mica support), for the appearance view.
    pub appearance: Arc<dyn SystemAppearance>,
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
            db,
            events,
        } = deps;
        Self {
            settings,
            consent,
            appearance,
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
 * WHAT:  `harness`: a CommandCtx over given settings and consent, a Mica-capable appearance fake, a fresh
 *        in-memory database with the real migrations and a RecordingSink for events, plus handles to the sink
 *        and the appearance fake; `ctx()` is the all-defaults one.
 * WHY:   Factory, command and app tests all need the same context without a Tauri app or a disk; keeping the
 *        builder here means a new CommandDeps field is added to tests in one place.
 * WHERE: Tests in ipc/factory.rs, ipc/commands and app/bindings.rs.
 */
#[cfg(test)]
pub mod testing {
    use std::sync::Arc;

    use super::{CommandCtx, CommandDeps};
    use crate::{
        ports::fakes::{FakePrivacyConsent, FakeSystemAppearance, RecordingSink},
        registry,
        services::Db,
        types::{AppEvent, SettingsSnapshot, SharedSettings},
    };

    /// A test context, the sink its events land in and its appearance fake (Mica, transparency on).
    pub struct Harness {
        pub ctx: CommandCtx,
        pub events: Arc<RecordingSink<AppEvent>>,
        pub appearance: Arc<FakeSystemAppearance>,
    }

    pub fn harness(settings: SettingsSnapshot, consent: FakePrivacyConsent) -> Harness {
        let events = Arc::new(RecordingSink::default());
        let appearance = Arc::new(FakeSystemAppearance::mica());
        let ctx = CommandCtx::new(CommandDeps {
            settings: SharedSettings::new(settings),
            consent: Arc::new(consent),
            appearance: Arc::clone(&appearance) as _,
            db: Db::open_in_memory().unwrap(),
            events: Arc::clone(&events) as _,
        });
        Harness {
            ctx,
            events,
            appearance,
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
