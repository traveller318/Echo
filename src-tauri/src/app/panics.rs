/*!
 * SOURCE OF TRUTH KEYWORDS: panic hook, fail take on panic, report panic to session, chained panic hook, PanicReporter, keep running after panic
 * WHAT:  `report_to_session`: chains a step onto the process panic hook that tells the session actor a panic
 *        happened, after the earlier hooks (the log line from app/logging.rs, then Rust's default) have run.
 * WHY:   02 §12: the panic hook logs the panic, marks the active take failed (audio kept) and the app keeps running
 *        where it can. Logging is installed first (before anything can panic); the session only exists later in
 *        bootstrap, so its step is chained on then instead of reaching into logging. The hook only posts a message
 *        (PanicReporter never blocks and holds a weak sender): the actor, the sole owner of recording state (02 §5),
 *        decides which take is live and fails it through its state machine. Keeping a panic from ending the app is
 *        done where the panic is caught (the command factory, the blocking pool, the ASR and capture workers, the
 *        session actor's own supervision); the hook cannot resume execution itself.
 * WHERE: Called once by app/bootstrap::start after the session handle exists.
 */

use std::panic;

use crate::pipeline::session::PanicReporter;

/// Adds "fail the live take" to the panic hook, after the hooks already installed.
pub fn report_to_session(reporter: PanicReporter) {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        previous(info);
        reporter.report();
    }));
}
