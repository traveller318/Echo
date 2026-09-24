/*!
 * SOURCE OF TRUTH KEYWORDS: bootstrap, composition root, startup sequence, resolve paths, open database, initial settings, appearance watcher, microphone adapter, command context
 * WHAT:  `start`: the startup sequence that runs before any window exists: resolve AppPaths from the Tauri path
 *        API, start local logging, open and migrate the database, resolve the stored settings over the registry
 *        defaults, start the appearance watcher, and manage the CommandCtx (settings; consent, appearance,
 *        launcher, microphone and thread-priority adapters; AppPaths, database, event sink).
 * WHY:   The composition root is the only place that names a concrete adapter or resolves a path (02 §3.2,
 *        05 W23); every other layer receives ports, AppPaths and the Db handle. Logging starts first so every
 *        later failure is on disk. It runs on the built app before `run_return`, so the CommandCtx is managed
 *        before the windows (created when the event loop starts) can invoke a command, and a failure ends startup
 *        with an exit code instead of a panic inside Tauri's setup hook. A service failure's detail is logged
 *        here, since no command factory is involved yet. The microphone adapter shares the consent adapter, so a
 *        blocked privacy switch is caught before any device opens (05 W13); nothing opens a device or loads ONNX
 *        Runtime at startup (05 W19). The session actor joins this sequence in step 14 and takes the same
 *        microphone and scheduler handles.
 * WHERE: Called once by app::run; its parts (TauriEventSink, logging) live next to it in app/.
 */

use std::{error::Error, sync::Arc};

use tauri::{App, Manager, Runtime};

use super::{events::TauriEventSink, logging};
use crate::{
    adapters::{
        appearance::Win32SystemAppearance, audio::CpalWasapiCapture, consent::Win32PrivacyConsent,
        launcher::Win32ShellLauncher, scheduler::Win32WorkerScheduler,
    },
    ipc::{CommandCtx, CommandDeps},
    pipeline::appearance::AppearanceRelay,
    ports::{EventSink, PrivacyConsent, SystemAppearance},
    registry,
    services::{self, Db},
    types::{AppEvent, AppPaths, PortError, SharedSettings},
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
    let settings = SharedSettings::new(registry::settings::resolve(stored));
    let events: Arc<dyn EventSink<AppEvent>> = Arc::new(TauriEventSink::new(app.handle().clone()));
    let appearance = Arc::new(Win32SystemAppearance::new());
    watch_appearance(appearance.as_ref(), &settings, &events);
    let consent: Arc<dyn PrivacyConsent> = Arc::new(Win32PrivacyConsent::new());
    app.manage(CommandCtx::new(CommandDeps {
        settings,
        audio: Arc::new(CpalWasapiCapture::new(Arc::clone(&consent))),
        consent,
        appearance,
        launcher: Arc::new(Win32ShellLauncher::new()),
        scheduler: Arc::new(Win32WorkerScheduler::new()),
        paths,
        db,
        events,
    }));
    tracing::info!("startup finished");
    Ok(())
}

/// Relays Windows transparency changes to the windows; without the watcher Echo still starts, and a change applies
/// on the next launch.
fn watch_appearance(
    appearance: &dyn SystemAppearance,
    settings: &SharedSettings,
    events: &Arc<dyn EventSink<AppEvent>>,
) {
    let relay = AppearanceRelay::new(settings.clone(), appearance.caps(), Arc::clone(events));
    if let Err(error) = appearance.listen(Arc::new(relay)) {
        tracing::warn!(
            detail = error.detail(),
            "transparency changes will apply after a restart"
        );
    }
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
