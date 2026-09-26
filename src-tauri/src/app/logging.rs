/*!
 * SOURCE OF TRUTH KEYWORDS: logging, tracing subscriber, JSON logs, rolling log files, daily rotation, log retention, panic hook, local log, debug log toggle, general.debug_log, follow_settings
 * WHAT:  `init`: installs the process-wide tracing subscriber (JSON lines at `info` into `logs/`, or `debug` while
 *        detailed logging is on, rolled daily, the newest 7 files kept; plus plain text on stderr in debug builds)
 *        and a panic hook that logs panics. `follow_settings`: switches detailed logging with the hidden
 *        `general.debug_log` setting (About → Troubleshooting), now and on every SettingsChanged.
 * WHY:   02 §12: observability is local only; there is no remote sink and no crash upload (02 §10). Lines carry
 *        ids, timings and error codes, never transcript text or audio: callers log only those, and the command
 *        factory records the command name and request id, never its input. The file writer is blocking rather
 *        than a background worker, so the lines just before a crash or exit are never lost; at `info` Echo writes
 *        a few lines per command or take, so the cost is negligible. The panic hook logs the message and location,
 *        then runs the previous hook, so a panic is on disk even when no console is attached. Failing the live take
 *        is chained on later by app/panics.rs, once the session exists (02 §12). The level is a switch the filter
 *        reads per event (one relaxed atomic load), not a rebuilt subscriber, so it changes live without a restart;
 *        debug lines carry the same ids and timings, never text. The switch follows the same SettingsChanged event
 *        the windows receive, so logging needs no port and no pipeline hook.
 * WHERE: `init` is called once by app/bootstrap before anything else can log (the file layout comes from AppPaths);
 *        `follow_settings` by app/bootstrap once the CommandCtx is managed.
 */

use std::{
    error::Error,
    fs, io, panic,
    path::Path,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use tauri::{AppHandle, Manager, Runtime};
use tauri_specta::Event;
use tracing::{Level, Metadata, Subscriber};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    Layer, filter::FilterFn, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::{
    ipc::CommandCtx,
    registry,
    types::{AppPaths, SettingsChanged},
};

/// Level written to the log files normally.
const FILE_LEVEL: Level = Level::INFO;

/// Level written while detailed logging is on.
const DETAILED_LEVEL: Level = Level::DEBUG;

/// The detailed-logging switch of the installed subscriber (set once by `init`).
static DETAILED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// Daily files kept before the oldest is deleted.
const KEPT_FILES: usize = 7;

/// Installs the subscriber and the panic hook; fails if the log folder cannot be used or a subscriber exists.
pub fn init(logs_dir: &Path) -> Result<(), Box<dyn Error>> {
    let detailed = Arc::clone(DETAILED.get_or_init(Arc::default));
    subscriber(appender(logs_dir)?, detailed).try_init()?;
    install_panic_hook();
    Ok(())
}

/**
 * SOURCE OF TRUTH KEYWORDS: follow_settings, debug log switch, detailed logging live, SettingsChanged listener
 * WHAT:  Applies `general.debug_log` to the log level now and after every settings change.
 * WHY:   The toggle takes effect at once (02 §12, "a debug toggle is hidden in About"); reading the live snapshot on
 *        each SettingsChanged keeps this free of any key matching. A change is logged at info, so the file shows
 *        where detailed lines begin and end.
 * WHERE: app/bootstrap `start`, after the CommandCtx is managed.
 */
pub fn follow_settings<R: Runtime>(app: &AppHandle<R>) {
    apply_detailed(app);
    let handle = app.clone();
    SettingsChanged::listen_any(app, move |_| apply_detailed(&handle));
}

fn apply_detailed<R: Runtime>(app: &AppHandle<R>) {
    let (Some(switch), Some(ctx)) = (DETAILED.get(), app.try_state::<CommandCtx>()) else {
        return;
    };
    let wanted = registry::settings::debug_log(&ctx.settings());
    if switch.swap(wanted, Ordering::Relaxed) != wanted {
        tracing::info!(detailed = wanted, "detailed logging switched");
    }
}

/// The rolling file writer in `logs_dir`, created first: the appender prunes old files at build time and
/// reports a missing folder on stderr instead of failing.
fn appender(logs_dir: &Path) -> Result<RollingFileAppender, Box<dyn Error>> {
    fs::create_dir_all(logs_dir)?;
    Ok(RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(AppPaths::LOG_FILE_PREFIX)
        .filename_suffix(AppPaths::LOG_FILE_SUFFIX)
        .max_log_files(KEPT_FILES)
        .build(logs_dir)?)
}

/// JSON lines into `file` at FILE_LEVEL, or DETAILED_LEVEL while `detailed` is on; in debug builds also readable
/// lines on stderr for `tauri dev`.
fn subscriber(
    file: RollingFileAppender,
    detailed: Arc<AtomicBool>,
) -> impl Subscriber + Send + Sync {
    let json = fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_writer(file)
        .with_filter(level_filter(Arc::clone(&detailed)));
    let console = cfg!(debug_assertions).then(|| {
        fmt::layer()
            .with_writer(io::stderr)
            .with_filter(level_filter(detailed))
    });
    tracing_subscriber::registry().with(json).with(console)
}

/// Passes events at FILE_LEVEL and above, and DETAILED_LEVEL ones while `detailed` is on.
fn level_filter(detailed: Arc<AtomicBool>) -> FilterFn<impl Fn(&Metadata<'_>) -> bool> {
    FilterFn::new(move |metadata: &Metadata<'_>| {
        let level = *metadata.level();
        level <= FILE_LEVEL || (level <= DETAILED_LEVEL && detailed.load(Ordering::Relaxed))
    })
}

fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()));
        tracing::error!(
            message = info.payload_as_str().unwrap_or("non-text panic payload"),
            location,
            "panic"
        );
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::testing::TempDir;

    #[test]
    fn events_are_written_as_json_lines_at_info_and_above() {
        let dir = TempDir::new("log-test");
        let logs = dir.join("logs");
        let quiet = Arc::new(AtomicBool::new(false));
        tracing::subscriber::with_default(subscriber(appender(&logs).unwrap(), quiet), || {
            let span = tracing::info_span!("command", command = "settings_set", request = 7_u64);
            span.in_scope(|| {
                tracing::info!(outcome = "ok", "command finished");
                tracing::debug!("hidden at info");
            });
        });

        let files: Vec<_> = fs::read_dir(&logs)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(
            files[0].starts_with("echo.") && files[0].ends_with(".log"),
            "{files:?}"
        );

        let text = fs::read_to_string(logs.join(&files[0])).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 1, "{text}");
        assert_eq!(lines[0]["level"], "INFO");
        assert_eq!(lines[0]["fields"]["outcome"], "ok");
        assert_eq!(lines[0]["span"]["command"], "settings_set");
        assert!(!text.contains("hidden at info"));
    }

    #[test]
    fn detailed_logging_adds_debug_lines_and_never_trace() {
        let dir = TempDir::new("log-detailed");
        let logs = dir.join("logs");
        let detailed = Arc::new(AtomicBool::new(false));
        let switch = Arc::clone(&detailed);
        tracing::subscriber::with_default(subscriber(appender(&logs).unwrap(), detailed), || {
            tracing::debug!("before the switch");
            switch.store(true, Ordering::Relaxed);
            tracing::debug!(take_ms = 120_u64, "per-take timing");
            tracing::trace!("never written");
            switch.store(false, Ordering::Relaxed);
            tracing::debug!("after the switch");
        });
        let file = fs::read_dir(&logs).unwrap().next().unwrap().unwrap().path();
        let text = fs::read_to_string(file).unwrap();
        assert!(text.contains("per-take timing"), "{text}");
        for hidden in ["before the switch", "never written", "after the switch"] {
            assert!(!text.contains(hidden), "{hidden} leaked into {text}");
        }
    }
}
