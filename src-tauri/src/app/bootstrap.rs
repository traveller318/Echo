/*!
 * SOURCE OF TRUTH KEYWORDS: bootstrap, composition root, startup sequence, resolve paths, open database, initial settings, managed state, command context
 * WHAT:  `start`: the startup sequence that runs before any window exists: resolve AppPaths from the Tauri path
 *        API, start local logging, open and migrate the database, resolve the stored settings over the registry
 *        defaults, and manage the CommandCtx (settings, consent adapter, database, event sink).
 * WHY:   The composition root is the only place that names a concrete adapter or resolves a path (02 §3.2,
 *        05 W23); every other layer receives ports, AppPaths and the Db handle. Logging starts first so every
 *        later failure is on disk. It runs on the built app before `run_return`, so the CommandCtx is managed
 *        before the windows (created when the event loop starts) can invoke a command, and a failure ends startup
 *        with an exit code instead of a panic inside Tauri's setup hook. A service failure's detail is logged
 *        here, since no command factory is involved yet. The session actor joins this sequence in step 14.
 * WHERE: Called once by app::run; its parts (TauriEventSink, logging) live next to it in app/.
 */

use std::{error::Error, sync::Arc};

use tauri::{App, Manager, Runtime};

use super::{events::TauriEventSink, logging};
use crate::{
    adapters::consent::Win32PrivacyConsent,
    ipc::{CommandCtx, CommandDeps},
    registry,
    services::{self, Db},
    types::{AppPaths, PortError, SharedSettings},
};

/// Resolves paths, starts logging, opens the database and manages the CommandCtx on `app`.
pub fn start<R: Runtime>(app: &App<R>) -> Result<(), Box<dyn Error>> {
    let resolver = app.path();
    let paths = AppPaths::new(resolver.app_local_data_dir()?, resolver.resource_dir()?);
    logging::init(&paths.logs_dir())?;
    tracing::info!(
        version = %app.package_info().version,
        "Echo is starting"
    );

    let db = Db::open(&paths).map_err(startup_failure)?;
    let stored = services::settings::get::all(&db).map_err(startup_failure)?;
    app.manage(CommandCtx::new(CommandDeps {
        settings: SharedSettings::new(registry::settings::resolve(stored)),
        consent: Arc::new(Win32PrivacyConsent::new()),
        db,
        events: Arc::new(TauriEventSink::new(app.handle().clone())),
    }));
    tracing::info!("startup finished");
    Ok(())
}

/// Logs a startup service failure with its internal detail and passes it on.
fn startup_failure(error: PortError) -> PortError {
    tracing::error!(
        code = error.error().code().as_str(),
        detail = error.detail(),
        "startup failed"
    );
    error
}
