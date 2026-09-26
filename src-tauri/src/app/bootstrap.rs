/*!
 * SOURCE OF TRUTH KEYWORDS: bootstrap, composition root, startup sequence, ASR worker, session actor, pill presenter, event fan-out, start_speech_engine, prepare_session, stop_session, command context, startup recovery, retention sweeper, panic hook, sound player, audio device watch, day watch, model manager, HTTP client
 * WHAT:  `start`: the startup sequence that runs before any window exists: resolve AppPaths from the Tauri path
 *        API, start local logging, open and migrate the database, settle the takes a crash left unfinished
 *        (pipeline/recovery.rs), resolve the stored settings over the registry defaults, start the appearance
 *        watcher and the microphone hot-plug watch (DeviceListRelay), start the (empty) ASR worker (its readiness
 *        relayed to the Models page and its load failures to the ModelWatch; its accelerator picker over the DXGI
 *        GPU list and the database), build the allowlisted HTTP client
 *        (network gate from the registry permission) and the model manager over HttpModelStore and the dialog
 *        plugin's folder picker, start the sound
 *        player, spawn the (idle) session actor over the same ports (with the sound cues) and point the
 *        panic hook at it (app/panics.rs), spawn the retention sweeper (first sweep now, then daily) and the
 *        dashboard's day watch (MetricsChanged when the local day changes), start the
 *        pill presenter over the overlay adapter, and manage the CommandCtx (settings;
 *        consent, appearance, launcher, microphone, thread-priority, hotkey, foreground-window, toast and
 *        main-window adapters; the ASR worker; the Delivery over the clipboard, paste and toast adapters; the
 *        session handle; the pill presenter; the retention handle; the disabled updater (02 §11); AppPaths,
 *        database, event sink) plus the overlay
 *        adapter itself, which app/windows.rs attaches to the pill window once it exists; returns the recovery report.
 *        `announce_recovery`, `start_speech_engine` and `prepare_session`: once the windows exist, toast what recovery
 *        found, load and warm the selected speech engine in the background and let the session bind its hotkeys.
 *        `stop_session`: at exit, finalize an open recording.
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
 *        are in front (05 W35). Recovery runs right after the database opens, before the session, the sweeper or
 *        any command exists, so no take can be live while stuck rows are settled; a recovery failure is logged and
 *        startup goes on (the next start retries), since the takes' audio stays on disk either way (02 §7.3).
 * WHERE: `start` is called once by app::run before the event loop, `start_speech_engine` and `prepare_session` on
 *        RunEvent::Ready, `stop_session` on RunEvent::Exit; its parts (TauriEventSink, logging) live next to it in
 *        app/.
 */

use std::{error::Error, sync::Arc, time::Duration};

use tauri::{App, AppHandle, Manager, Runtime};

use super::{events::TauriEventSink, logging, panics, windows::MAIN_WINDOW};
use crate::{
    adapters::{
        appearance::Win32SystemAppearance,
        audio::CpalWasapiCapture,
        clipboard::ArboardClipboard,
        consent::Win32PrivacyConsent,
        dialog::TauriFolderPicker,
        foreground::Win32ForegroundApp,
        gpu::DxgiGraphicsAdapters,
        hotkey::LowLevelKeyboardHotkeys,
        inserter::Win32SendInputInserter,
        launcher::Win32ShellLauncher,
        net::{HttpClient, HttpModelStore},
        notifier::TauriToastNotifier,
        scheduler::Win32WorkerScheduler,
        sound::Win32SoundPlayer,
        updater::DisabledUpdater,
        window::{TauriMainWindow, Win32OverlayWindow},
    },
    ipc::{CommandCtx, CommandDeps},
    pipeline::{
        appearance::AppearanceRelay,
        asr::{self, AcceleratorPicker, AsrWorker, AsrWorkerConfig},
        audio_devices::{DEVICE_SETTLE, DeviceListRelay},
        delivery::{Delivery, DeliveryPorts},
        fan_out::FanOut,
        metrics::DayWatch,
        models::{ModelDeps, ModelManager, ModelPolicy, ModelWatch, ReadinessRelay},
        pill::{PillPresenter, PillTiming},
        recovery,
        retention::{RetentionDeps, RetentionHandle},
        session::{SessionActor, SessionConfig, SessionEngines, SessionHandle},
        sound_cues::SoundCues,
    },
    ports::{
        AudioCapture, EventSink, ForegroundApp, HotkeyService, Notifier, PrivacyConsent,
        SystemAppearance, WorkerScheduler,
    },
    registry::{self, engines::BuildCtx},
    services::{self, Db},
    types::{
        AcceleratorPolicy, AppEvent, AppPaths, Permission, PortError, RecoveryReport,
        SharedSettings,
    },
};

/// Resolves paths, starts logging, opens the database, settles the takes a crash left unfinished and manages the
/// CommandCtx on `app`; returns what recovery found, for `announce_recovery` once the windows exist.
pub fn start<R: Runtime>(app: &App<R>) -> Result<RecoveryReport, Box<dyn Error>> {
    let resolver = app.path();
    let paths = AppPaths::new(resolver.app_local_data_dir()?, resolver.resource_dir()?);
    logging::init(&paths.logs_dir())?;
    tracing::info!(
        version = %app.package_info().version,
        "Echo is starting"
    );
    let db = Db::open(&paths).map_err(startup_failure)?;
    // Before any window, command or take exists, so nothing can race it for a row (02 §7.3 step 4).
    let recovered = recovery::recover(&db, &paths).unwrap_or_else(|error| {
        tracing::error!(
            code = error.error().code().as_str(),
            detail = error.detail(),
            "startup recovery could not list unfinished takes; the next start tries again"
        );
        RecoveryReport::default()
    });
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
    // The worker reports readiness to the Models page and its load failures to the model check (ModelWatch).
    let (readiness, load_failures) = ReadinessRelay::new(Arc::clone(&events));
    // Where each engine runs: DXGI lists the GPUs, the database remembers `auto` measurements (02 §8.1).
    let accelerators = AcceleratorPicker::new(
        Arc::new(DxgiGraphicsAdapters::new()),
        db.clone(),
        AcceleratorPolicy::DEFAULT,
    );
    let asr = AsrWorker::spawn(AsrWorkerConfig::registry(
        BuildCtx {
            paths: paths.clone(),
        },
        accelerators,
        Arc::clone(&scheduler),
        Some(Arc::new(readiness)),
    ))
    .map_err(startup_failure)?;
    // Built before the model manager, which releases and warms the polish chain around its installs.
    let engines = SessionEngines::registry(BuildCtx {
        paths: paths.clone(),
    });
    let http = HttpClient::new(
        registry::network::download_allowlist(),
        registry::network::HTTP_POLICY,
        registry::permissions::gate(Permission::Network, settings.clone(), Arc::clone(&consent)),
    )
    .map_err(startup_failure)?;
    let models = ModelManager::new(
        ModelDeps {
            store: Arc::new(HttpModelStore::new(http, paths.clone())),
            picker: Arc::new(TauriFolderPicker::new(app.handle().clone(), MAIN_WINDOW)),
            asr: asr.clone(),
            polish: engines.polish.clone(),
            settings: settings.clone(),
            consent: Arc::clone(&consent),
            paths: paths.clone(),
            events: Arc::clone(&events),
        },
        ModelPolicy::DEFAULT,
    );
    // Hashes a model whose engine failed to load (02 §8.2); runs until the ASR worker shuts down.
    tauri::async_runtime::spawn(ModelWatch::new(models.clone(), load_failures).run());
    let notifier: Arc<dyn Notifier> = Arc::new(TauriToastNotifier::new(app.handle().clone()));
    let delivery = Delivery::new(DeliveryPorts {
        clipboard: Arc::new(ArboardClipboard::new()),
        inserter: Arc::new(Win32SendInputInserter::new()),
        notifier: Arc::clone(&notifier),
    });
    let audio: Arc<dyn AudioCapture> = Arc::new(CpalWasapiCapture::new(Arc::clone(&consent)));
    watch_audio_devices(&audio, &events);
    let sounds = SoundCues::new(Arc::new(Win32SoundPlayer::new().map_err(startup_failure)?));
    let hotkeys: Arc<dyn HotkeyService> = Arc::new(LowLevelKeyboardHotkeys::new());
    let (session, inbox) = SessionHandle::new();
    panics::report_to_session(session.panic_reporter());
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
            engines: engines.clone(),
            sounds,
        },
        inbox,
    );
    // Nothing is opened or bound until `prepare_session` runs on RunEvent::Ready.
    tauri::async_runtime::spawn(actor.run());
    let (retention, sweeper) = RetentionHandle::new(RetentionDeps {
        settings: settings.clone(),
        paths: paths.clone(),
        db: db.clone(),
        events: Arc::clone(&events),
    });
    // Its first sweep runs now, on the blocking pool, after recovery settled every unfinished take.
    tauri::async_runtime::spawn(sweeper.run());
    // Refreshes the dashboard's "today" when the local day changes; runs until the runtime stops.
    tauri::async_runtime::spawn(DayWatch::new(db.clone(), Arc::clone(&events)).run());
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
        engines,
        pill,
        main_window: Arc::new(TauriMainWindow::new(app.handle().clone(), MAIN_WINDOW)),
        retention,
        models,
        updater: Arc::new(DisabledUpdater::new()),
        paths,
        db,
        events,
    }));
    tracing::info!("startup finished");
    Ok(recovered)
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

/**
 * SOURCE OF TRUTH KEYWORDS: announce_recovery, Recovered N takes toast, recovery toast on ready
 * WHAT:  Shows the startup recovery's toasts and tells the windows History changed.
 * WHY:   Recovery runs before any window exists (so nothing races it), but a toast raised before the shell and the
 *        event loop are up can be lost (05 W19), so it is shown on RunEvent::Ready instead.
 * WHERE: app::run on the first RunEvent::Ready, with the report `start` returned.
 */
pub fn announce_recovery<R: Runtime>(app: &AppHandle<R>, report: RecoveryReport) {
    match app.try_state::<CommandCtx>() {
        Some(ctx) => recovery::announce(report, ctx.notifier(), ctx.events()),
        None => tracing::error!("recovery was announced before the command context was managed"),
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

/**
 * SOURCE OF TRUTH KEYWORDS: watch_audio_devices, microphone hot-plug wiring, DeviceListRelay, watch_devices
 * WHAT:  Starts the relay that turns the capture adapter's endpoint notices into AudioDevicesChanged, and hands it
 *        to the adapter's device watch.
 * WHY:   Registering for notices opens no device (05 W19 still holds: the microphone opens on the first take). Without
 *        the watch Echo still works: the Settings list is read whenever it opens and every take resolves its device
 *        afresh, so a failure is only logged.
 * WHERE: `start`, right after the capture adapter exists.
 */
fn watch_audio_devices(audio: &Arc<dyn AudioCapture>, events: &Arc<dyn EventSink<AppEvent>>) {
    let watched = DeviceListRelay::spawn(Arc::clone(audio), Arc::clone(events), DEVICE_SETTLE)
        .and_then(|relay| audio.watch_devices(Arc::new(relay)));
    if let Err(error) = watched {
        tracing::warn!(
            detail = error.detail(),
            "microphone changes will show when the Settings list is opened"
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
