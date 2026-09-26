/*!
 * SOURCE OF TRUTH KEYWORDS: ProcessStats, process memory, memory use, working set, private bytes, About memory
 * WHAT:  ProcessStats reports how much memory Echo's own process uses right now (ProcessMemory).
 * WHY:   The speech model keeps about 1 GB resident on purpose (05 A7), and About shows it so the user can see
 *        what Echo costs; reading a process's memory is an OS call, so it sits behind a port.
 * WHERE: Implemented by adapters/process/win32.rs (Win32ProcessStats) and ports/fakes; called by `app_about`
 *        (ipc/commands/system.rs).
 */

use crate::types::{PortResult, ProcessMemory};

/// Facts about Echo's own process.
pub trait ProcessStats: Send + Sync {
    fn memory(&self) -> PortResult<ProcessMemory>;
}
