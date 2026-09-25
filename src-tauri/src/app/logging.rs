/*!
 * SOURCE OF TRUTH KEYWORDS: logging, tracing subscriber, JSON logs, rolling log files, daily rotation, log retention, panic hook, local log
 * WHAT:  `init`: installs the process-wide tracing subscriber (JSON lines at `info` into `logs/`, rolled daily,
 *        the newest 7 files kept; plus plain text on stderr in debug builds) and a panic hook that logs panics.
 * WHY:   02 §12: observability is local only; there is no remote sink and no crash upload (02 §10). Lines carry
 *        ids, timings and error codes, never transcript text or audio: callers log only those, and the command
 *        factory records the command name and request id, never its input. The file writer is blocking rather
 *        than a background worker, so the lines just before a crash or exit are never lost; at `info` Echo writes
 *        a few lines per command or take, so the cost is negligible. The panic hook logs the message and location,
 *        then runs the previous hook, so a panic is on disk even when no console is attached. Failing the live take
 *        is chained on later by app/panics.rs, once the session exists (02 §12).
 * WHERE: Called once by app/bootstrap before anything else can log; the file layout comes from AppPaths.
 */

use std::{error::Error, fs, io, panic, path::Path};

use tracing::{Subscriber, level_filters::LevelFilter};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::types::AppPaths;

/// Level written to the log files (a debug switch joins About in a later step).
const FILE_LEVEL: LevelFilter = LevelFilter::INFO;

/// Daily files kept before the oldest is deleted.
const KEPT_FILES: usize = 7;

/// Installs the subscriber and the panic hook; fails if the log folder cannot be used or a subscriber exists.
pub fn init(logs_dir: &Path) -> Result<(), Box<dyn Error>> {
    subscriber(appender(logs_dir)?).try_init()?;
    install_panic_hook();
    Ok(())
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

/// JSON lines at FILE_LEVEL into `file`; in debug builds also readable lines on stderr for `tauri dev`.
fn subscriber(file: RollingFileAppender) -> impl Subscriber + Send + Sync {
    let json = fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(false)
        .with_writer(file)
        .with_filter(FILE_LEVEL);
    let console = cfg!(debug_assertions)
        .then(|| fmt::layer().with_writer(io::stderr).with_filter(FILE_LEVEL));
    tracing_subscriber::registry().with(json).with(console)
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
        tracing::subscriber::with_default(subscriber(appender(&logs).unwrap()), || {
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
}
