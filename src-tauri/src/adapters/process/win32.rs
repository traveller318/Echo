/*!
 * SOURCE OF TRUTH KEYWORDS: Win32ProcessStats, GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX, WorkingSetSize, PrivateUsage, memory use
 * WHAT:  Win32ProcessStats: ProcessStats through GetProcessMemoryInfo on Echo's own process: the working set (what
 *        Task Manager shows) and the private usage (commit only Echo holds).
 * WHY:   About shows what Echo costs (05 A7). The pseudo-handle of the current process needs no opening or closing
 *        and always has the query right; the EX structure is asked for so private usage comes in the same call.
 * WHERE: Built by app/bootstrap into CommandCtx; called by `app_about`.
 */

use windows::Win32::System::{
    ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
    Threading::GetCurrentProcess,
};

use crate::{
    ports::ProcessStats,
    types::{AppError, ByteCount, PortError, PortResult, ProcessMemory},
};

/// Echo's own process through the Win32 process status API.
#[derive(Debug, Default, Clone, Copy)]
pub struct Win32ProcessStats;

impl Win32ProcessStats {
    pub const fn new() -> Self {
        Self
    }
}

impl ProcessStats for Win32ProcessStats {
    fn memory(&self) -> PortResult<ProcessMemory> {
        let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
        let size = u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS_EX>()).unwrap_or(u32::MAX);
        // SAFETY: the pseudo-handle is always valid; `counters` is an EX structure of `size` bytes, which the API
        // accepts through the base structure pointer (the EX layout starts with the base fields).
        unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
                size,
            )
        }
        .map_err(|error| {
            PortError::new(AppError::Internal)
                .with_detail(format!("GetProcessMemoryInfo failed: {error}"))
        })?;
        Ok(ProcessMemory {
            working_set: bytes(counters.WorkingSetSize),
            private_bytes: bytes(counters.PrivateUsage),
        })
    }
}

fn bytes(count: usize) -> ByteCount {
    ByteCount::new(u64::try_from(count).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_plausible_footprint_for_this_process() {
        let memory = Win32ProcessStats::new().memory().unwrap();
        assert!(memory.working_set.get() > 1024 * 1024, "{memory:?}");
        assert!(memory.private_bytes.get() > 0, "{memory:?}");
    }
}
