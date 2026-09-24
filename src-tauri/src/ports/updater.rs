/*!
 * SOURCE OF TRUTH KEYWORDS: Updater, update check, install update, DisabledUpdater, UpdaterCaps, no network update
 * WHAT:  Updater checks for and installs a newer Echo.
 * WHY:   This build ships without an update source (02 §11): the DisabledUpdater declares
 *        `UpdaterCaps.available: false`, answers `check` with `not_configured` and makes no network call. A future
 *        source is one new adapter. Installing replaces the running app, so the pipeline never calls `install`
 *        while a take is active.
 * WHERE: Implemented by adapters/updater/disabled.rs (DisabledUpdater) and ports/fakes; called by the
 *        `updates_check` / `updates_install` commands.
 */

use crate::types::{BoxFuture, PortResult, UpdateStatus, UpdaterCaps};

/// App updates.
pub trait Updater: Send + Sync {
    fn caps(&self) -> UpdaterCaps;

    fn check(&self) -> BoxFuture<'_, PortResult<UpdateStatus>>;

    /// Installs the update `check` found. Fails with `NotFound { update }` when there is none.
    fn install(&self) -> BoxFuture<'_, PortResult<()>>;
}
