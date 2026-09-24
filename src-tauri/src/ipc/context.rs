/*!
 * SOURCE OF TRUTH KEYWORDS: CommandCtx, command context, managed state, handler dependencies, SharedSettings, PrivacyConsent, reentrancy locks
 * WHAT:  CommandCtx: everything a command handler and the factory pipeline may use, managed once by Tauri and
 *        passed to every handler as `&CommandCtx`.
 * WHY:   Handlers take their dependencies from one place instead of Tauri state lookups, so they stay plain async
 *        fns that tests call with a context built from port fakes. It holds only ports (`Arc<dyn …>`, never an
 *        adapter, 02 §3.2), shared handles and the factory's own reentrancy locks; the registry needs no handle
 *        because it is compiled-in `const` data. Fields are added as their layers arrive: the database handle
 *        with services (step 06), the session actor's inbox with the pipeline (step 14).
 * WHERE: Built by app/bootstrap and managed on the Tauri app; read by ipc/factory.rs (preflight, reentrancy)
 *        and every handler in ipc/commands.
 */

use std::sync::Arc;

use super::reentrancy::ReentrancyLocks;
use crate::{
    ports::PrivacyConsent,
    types::{SettingsSnapshot, SharedSettings},
};

/// What command handlers and the factory may use.
pub struct CommandCtx {
    settings: SharedSettings,
    consent: Arc<dyn PrivacyConsent>,
    locks: ReentrancyLocks,
}

impl CommandCtx {
    pub fn new(settings: SharedSettings, consent: Arc<dyn PrivacyConsent>) -> Self {
        Self {
            settings,
            consent,
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

    /// The factory's keyed reentrancy locks.
    pub(super) fn locks(&self) -> &ReentrancyLocks {
        &self.locks
    }
}
