/*!
 * SOURCE OF TRUTH KEYWORDS: AboutView, AppInfo, ProcessMemory, app_about, version, development build, memory use, working set, private bytes
 * WHAT:  AppInfo (Echo's version and whether this is a development build), ProcessMemory (the memory Echo's
 *        process uses right now) and AboutView, what Settings → About shows besides the engine and the models.
 * WHY:   The version is the one Tauri read from tauri.conf.json at build time, so About can never disagree with the
 *        installer; only the composition root knows it, so it travels in AppInfo. Memory is shown because the speech
 *        model keeps about 1 GB resident on purpose (05 A7) and a user should be able to see that; the working set is
 *        what Task Manager shows, private bytes what Echo alone holds. Memory is best effort: None when Windows
 *        cannot answer, never a guess.
 * WHERE: AppInfo is built by app/bootstrap and held by CommandCtx; ProcessMemory by `ProcessStats::memory`
 *        (ports/process_stats.rs); AboutView is returned by `app_about` (ipc/commands/system.rs) to
 *        src/routes/settings (About).
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::ByteCount;

/// What this build of Echo is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct AppInfo {
    /// SemVer from tauri.conf.json, e.g. `0.1.0`.
    pub version: String,
    /// A development build (`tauri dev`), which loads its pages from the dev server.
    pub development: bool,
}

/// Memory the Echo process uses now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct ProcessMemory {
    /// Physical memory in use (Task Manager's "Memory").
    pub working_set: ByteCount,
    /// Memory only Echo holds (committed private bytes).
    pub private_bytes: ByteCount,
}

/// Settings → About, besides the speech engine (`engine_status`) and the models (`models_list`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct AboutView {
    pub app: AppInfo,
    /// None when Windows could not report it.
    pub memory: Option<ProcessMemory>,
}
