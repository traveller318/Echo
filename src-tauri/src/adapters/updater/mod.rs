/*!
 * SOURCE OF TRUTH KEYWORDS: updater adapters, Updater implementations, DisabledUpdater, update source
 * WHAT:  Adapters behind the Updater port.
 * WHY:   This build ships without an update source (02 §11); a future source (an update endpoint) is one more
 *        adapter here selected in app/, with no change to the core, which reads only `UpdaterCaps`.
 * WHERE: Constructed by app/bootstrap; used only through `dyn Updater`.
 */

mod disabled;

pub use disabled::DisabledUpdater;
