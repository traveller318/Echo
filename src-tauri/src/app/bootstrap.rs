/*!
 * SOURCE OF TRUTH KEYWORDS: bootstrap, composition root, startup sequence, ASR worker, session actor, pill presenter, event fan-out, start_speech_engine, prepare_session, stop_session, command context
 * WHAT:  `start`: the startup sequence that runs before any window exists: resolve AppPaths from the Tauri path
 *        API, start local logging, open and migrate the database, resolve the stored settings over the registry
 *        defaults, start the appearance watcher, start the (empty) ASR worker, spawn the (idle) session actor over
 *        the same ports, start the pill presenter over the overlay adapter, and manage the CommandCtx (settings;
 *        consent, appearance, launcher, microphone, thread-priority, hotkey, foreground-window, toast and
 *        main-window adapters; the ASR worker; the Delivery over the clipboard, paste and toast adapters; the
 *        session handle; the pill presenter; AppPaths, database, event sink) plus the overlay adapter itself, which
 *        app/windows.rs attaches to the pill window once it exists.
 *        `start_speech_engine` and `prepare_session`: once the windows exist, load and warm the selected speech
 *        engine in the background and let the session bind its hotkeys. `stop_session`: at exit, finalize an open
 *        recording.
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
 *        (onboarding and the Models page offer the download). The session actor takes the same microphone,
 *        scheduler, ASR worker, hotkey, foreground, toast and delivery handles as the commands (one instance of
 *        each); it is spawned idle on Tauri's runtime and binds nothing until `prepare_session`, so no combination is
 *        taken from other apps before Echo can act on it (the keyboard hook is installed with the first binding).
 *        The toast adapter looks up its plugin's state, which app/plugins.rs registered before build. Every event
 *        goes through one FanOut: the pill presenter first (so the window starts showing before the page renders),
 *        then the windows; the presenter observes the session through it and the actor never knows about windows.
 *        The process opts out of Windows power throttling first thing, because Echo does its work while other apps
 *        are in front (05 W35).
 * WHERE: `start` is called once by app::run before the event loop, `start_speech_engine` and `prepare_session` on
 *        RunEvent::Ready, `stop_session` on RunEvent::Exit; its parts (TauriEventSink, logging) live next to it in
 *        app/.
 */

use std::{error::Error, sync::Arc, time::Duration};

use tauri::{App, AppHandle, Manager, Runtime};

use super::{events::TauriEventSink, logging, windows::MAIN_WINDOW};
use crate::{
    adapters::{
        appearance::Win32SystemAppearance,
        audio::CpalWasapiCapture,
        clipboard::ArboardClipboard,
        consent::Win32PrivacyConsent,
        foreground::Win32ForegroundApp,
        hotkey::LowLevelKeyboardHotkeys,
        inserter::Win32SendInputInserter,
        launcher::Win32ShellLauncher,
        notifier::TauriToastNotifier,
        scheduler::Win32WorkerScheduler,
        window::{TauriMainWindow, Win32OverlayWindow},
    },
    ipc::{CommandCtx, CommandDeps},
    pipeline::{
        appearance::AppearanceRelay,
        asr::{self, AsrWorker, AsrWorkerConfig},
        delivery::{Delivery, DeliveryPorts},
        fan_out::FanOut,
        pill::{PillPresenter, PillTiming},
        session::{SessionActor, SessionConfig, SessionEngines, SessionHandle},
    },
    ports::{
        AudioCapture, EventSink, ForegroundApp, HotkeyService, Notifier, PrivacyConsent,
        SystemAppearance, WorkerScheduler,
    },
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
    let foreground: Arc<dyn ForegroundApp> = Arc::new(Win32ForegroundApp::new());
    let overlay = Arc::new(Win32OverlayWindow::new());
    let pill = PillPresenter::spawn(
        Arc::clone(&overlay) as _,
        Arc::clone(&foreground),
        PillTiming::DEFAULT,
    )
    .map_err(startup_failure)?;
    let events: Arc<dyn EventSink<AppEvent>> = Arc::new(FanOut::new(vec![
        Arc::new(pill.clone()),
        Arc::new(TauriEventSink::new(app.handle().clone())),
    ]));
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
    let audio: Arc<dyn AudioCapture> = Arc::new(CpalWasapiCapture::new(Arc::clone(&consent)));
    let hotkeys: Arc<dyn HotkeyService> = Arc::new(LowLevelKeyboardHotkeys::new());
    let (session, inbox) = SessionHandle::new();
    let actor = SessionActor::new(
        SessionConfig {
            settings: settings.clone(),
            audio: Arc::clone(&audio),
            scheduler: Arc::clone(&scheduler),
            asr: asr.clone(),
            hotkeys: Arc::clone(&hotkeys),
            foreground: Arc::clone(&foreground),
            notifier: Arc::clone(&notifier),
            delivery: delivery.clone(),
            paths: paths.clone(),
            db: db.clone(),
            events: Arc::clone(&events),
            engines: SessionEngines::registry(BuildCtx {
                paths: paths.clone(),
            }),
        },
        inbox,
    );
    // Nothing is opened or bound until `prepare_session` runs on RunEvent::Ready.
    tauri::async_runtime::spawn(actor.run());
    // The pill window exists only once the event loop runs; app/windows.rs attaches it then.
    app.manage(overlay);
    app.manage(CommandCtx::new(CommandDeps {
        settings,
        audio,
        consent,
        appearance,
        launcher: Arc::new(Win32ShellLauncher::new()),
        scheduler,
        asr,
        hotkeys,
        foreground,
        notifier,
        delivery,
        session,
        pill,
        main_window: Arc::new(TauriMainWindow::new(app.handle().clone(), MAIN_WINDOW)),
        paths,
        db,
        events,
    }));
    tracing::info!("startup finished");
    Ok(())
}

/**
 * SOURCE OF TRUTH KEYWORDS: prepare_session, bind record hotkey, warm voice detector, session ready
 * WHAT:  Tells the session actor the windows exist: it starts listening to hotkeys, binds the record hotkey and
 *        warms the voice detector and the polish chain.
 * WHY:   No hotkey is taken from other apps and no device or ONNX Runtime is touched before the UI is up (05 W19);
 *        the keyboard hook is installed with the first binding, so no key is watched before Echo can act on it.
 * WHERE: app::run on RunEvent::Ready, after start_speech_engine.
 */
pub fn prepare_session<R: Runtime>(app: &AppHandle<R>) {
    match app.try_state::<CommandCtx>() {
        Some(ctx) => ctx.session().prepare(),
        None => tracing::error!("the session was prepared before the command context was managed"),
    }
}

/// How long exit waits for the session to finalize an open recording (a WAV header rewrite takes milliseconds).
const SESSION_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/**
 * SOURCE OF TRUTH KEYWORDS: stop_session, exit during recording, finalize WAV on exit, graceful shutdown
 * WHAT:  Asks the session actor to finalize any open recording and stop, waiting a bounded time.
 * WHY:   02 §5: an app shutdown during Recording finalizes the WAV header before exit, so the take is complete on
 *        disk for startup recovery; a hung device must not keep the process alive, hence the timeout.
 * WHERE: app::run on RunEvent::Exit.
 */
pub fn stop_session<R: Runtime>(app: &AppHandle<R>) {
    let Some(ctx) = app.try_state::<CommandCtx>() else {
        return;
    };
    if !ctx.session().shutdown(SESSION_SHUTDOWN_TIMEOUT) {
        tracing::warn!("the session did not finish closing before exit");
    }
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
