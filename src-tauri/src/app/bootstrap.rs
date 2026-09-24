/*!
 * SOURCE OF TRUTH KEYWORDS: bootstrap, composition root, startup sequence, resolve paths, open database, initial settings, appearance watcher, microphone adapter, ASR worker, delivery adapters, hotkey adapter, start_speech_engine, command context
 * WHAT:  `start`: the startup sequence that runs before any window exists: resolve AppPaths from the Tauri path
 *        API, start local logging, open and migrate the database, resolve the stored settings over the registry
 *        defaults, start the appearance watcher, start the (empty) ASR worker, and manage the CommandCtx (settings;
 *        consent, appearance, launcher, microphone, thread-priority, hotkey, foreground-window and toast adapters;
 *        the ASR worker; the Delivery over the clipboard, paste and toast adapters; AppPaths, database, event
 *        sink). `start_speech_engine`: once the windows exist, load and warm the selected speech engine in
 *        the background.
 * WHY:   The composition root is the only place that names a concrete adapter or resolves a path (02 §3.2,
 *        05 W23); every other layer receives ports, AppPaths and the Db handle. Logging starts first so every
 *        later failure is on disk. It runs on the built app before `run_return`, so the CommandCtx is managed
 *        before the windows (created when the event loop starts) can invoke a command, and a failure ends startup
 *        with an exit code instead of a panic inside Tauri's setup hook. A service failure's detail is logged
 *        here, since no command factory is involved yet. The microphone adapter shares the consent adapter, so a
 *        blocked privacy switch is caught before any device opens (05 W13); nothing opens a device or loads ONNX
 *        Runtime before the windows exist (05 W19). The speech engine (about 1 GB, seconds to load, 05 A7–A8) is
 *        loaded only after the UI is up and on the worker's own loader thread, so the first paint never waits for
 *        it (02 §6.1 "resident, warm model"); a missing model is logged and reported by the worker's readiness
 *        (onboarding and the Models page offer the download). The session actor joins this sequence in step 14 and
 *        takes the same microphone, scheduler, ASR worker, hotkey, foreground and delivery handles; no hotkey is
 *        registered before it exists, so no combination is taken from other apps while pressing it would do nothing.
 *        The hotkey and toast adapters look up their plugins' state, which app/plugins.rs registered before build. The
 *        process opts out of Windows power throttling first thing, because Echo does its work while other apps are in
 *        front (05 W35).
 * WHERE: `start` is called once by app::run before the event loop, `start_speech_engine` on RunEvent::Ready; its
 *        parts (TauriEventSink, logging) live next to it in app/.
 */

use std::{error::Error, sync::Arc};

use tauri::{App, AppHandle, Manager, Runtime};

use super::{events::TauriEventSink, logging};
use crate::{
    adapters::{
        appearance::Win32SystemAppearance, audio::CpalWasapiCapture, clipboard::ArboardClipboard,
        consent::Win32PrivacyConsent, foreground::Win32ForegroundApp, hotkey::TauriGlobalShortcut,
        inserter::Win32SendInputInserter, launcher::Win32ShellLauncher,
        notifier::TauriToastNotifier, scheduler::Win32WorkerScheduler,
    },
    ipc::{CommandCtx, CommandDeps},
    pipeline::{
        appearance::AppearanceRelay,
        asr::{self, AsrWorker, AsrWorkerConfig},
        delivery::{Delivery, DeliveryPorts},
    },
    ports::{EventSink, Notifier, PrivacyConsent, SystemAppearance, WorkerScheduler},
    registry::{self, engines::BuildCtx},
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
    let scheduler: Arc<dyn WorkerScheduler> = Arc::new(Win32WorkerScheduler::new());
    if let Err(error) = scheduler.keep_full_speed() {
        tracing::warn!(
            detail = error.detail(),
            "Windows may slow Echo down while it works in the background"
        );
    }
    let asr = AsrWorker::spawn(AsrWorkerConfig::registry(
        BuildCtx {
            paths: paths.clone(),
        },
        Arc::clone(&scheduler),
        None,
    ))
    .map_err(startup_failure)?;
    let notifier: Arc<dyn Notifier> = Arc::new(TauriToastNotifier::new(app.handle().clone()));
    let delivery = Delivery::new(DeliveryPorts {
        clipboard: Arc::new(ArboardClipboard::new()),
        inserter: Arc::new(Win32SendInputInserter::new()),
        notifier: Arc::clone(&notifier),
    });
    app.manage(CommandCtx::new(CommandDeps {
        settings,
        audio: Arc::new(CpalWasapiCapture::new(Arc::clone(&consent))),
        consent,
        appearance,
        launcher: Arc::new(Win32ShellLauncher::new()),
        scheduler,
        asr,
        hotkeys: Arc::new(TauriGlobalShortcut::new(app.handle().clone())),
        foreground: Arc::new(Win32ForegroundApp::new()),
        notifier,
        delivery,
        paths,
        db,
        events,
    }));
    tracing::info!("startup finished");
    Ok(())
}

/**
 * SOURCE OF TRUTH KEYWORDS: start_speech_engine, startup engine load, background warm-up, load_request
 * WHAT:  Resolves the engine the settings select into a load request and hands it to the ASR worker, which loads
 *        and warms it on its own thread; returns at once.
 * WHY:   Called when the windows exist, so model loading never delays the first paint. The outcome is logged by the
 *        worker and visible through its readiness; nothing waits on it here. A settings value that names no ASR
 *        engine is logged, and takes then fail with that error instead of Echo refusing to start.
 * WHERE: app::run on RunEvent::Ready, after windows::setup.
 */
pub fn start_speech_engine<R: Runtime>(app: &AppHandle<R>) {
    let Some(ctx) = app.try_state::<CommandCtx>() else {
        tracing::error!("the speech engine was started before the command context was managed");
        return;
    };
    match asr::load_request(&ctx.settings(), ctx.paths()) {
        Ok(request) => {
            tracing::info!(engine = %request.engine_id, accelerator = ?request.accelerator, "loading the speech engine");
            // The outcome is logged by the worker and read through its readiness; nobody waits for it here.
            drop(ctx.asr().load(request));
        }
        Err(error) => tracing::error!(
            code = error.error().code().as_str(),
            detail = error.detail(),
            "no speech engine to load"
        ),
    }
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
