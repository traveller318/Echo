/*!
 * SOURCE OF TRUTH KEYWORDS: LinuxProcessStats, /proc/self/statm, resident set, private memory
 * WHAT:  LinuxProcessStats: ProcessStats from `/proc/self/statm`: the resident set as the working set, and resident
 *        minus shared pages as the private bytes.
 * WHY:   statm is one short line with no parsing ambiguity; it counts pages, so the page size comes from sysconf.
 *        These are the closest Linux equivalents of the Windows working set and private bytes the About page shows.
 * WHERE: Built by app/bootstrap on Linux into CommandCtx; read by the About page.
 */

use crate::{
    ports::ProcessStats,
    types::{AppError, ByteCount, PortError, PortResult, ProcessMemory},
};

#[derive(Debug, Default)]
pub struct LinuxProcessStats;

impl LinuxProcessStats {
    pub const fn new() -> Self {
        Self
    }
}

impl ProcessStats for LinuxProcessStats {
    fn memory(&self) -> PortResult<ProcessMemory> {
        let statm = std::fs::read_to_string("/proc/self/statm").map_err(|error| {
            PortError::new(AppError::Internal).with_detail(format!("/proc/self/statm: {error}"))
        })?;
        // SAFETY: sysconf only reads a configuration value.
        let page = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).unwrap_or(4096);
        parse_statm(&statm, page).ok_or_else(|| {
            PortError::new(AppError::Internal).with_detail(format!("unexpected statm: {statm:?}"))
        })
    }
}

/// `size resident shared …` in pages → working set (resident) and private bytes (resident − shared).
fn parse_statm(statm: &str, page: u64) -> Option<ProcessMemory> {
    let mut fields = statm.split_whitespace().skip(1).map(str::parse::<u64>);
    let resident = fields.next()?.ok()?;
    let shared = fields.next()?.ok()?;
    Some(ProcessMemory {
        working_set: ByteCount::new(resident.saturating_mul(page)),
        private_bytes: ByteCount::new(resident.saturating_sub(shared).saturating_mul(page)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statm_pages_become_bytes() {
        let memory = parse_statm("1000 300 100 5 0 200 0\n", 4096).unwrap();
        assert_eq!(memory.working_set.get(), 300 * 4096);
        assert_eq!(memory.private_bytes.get(), 200 * 4096);
        assert!(parse_statm("garbage", 4096).is_none());
    }

    #[test]
    fn this_process_has_memory() {
        assert!(LinuxProcessStats::new().memory().unwrap().working_set.get() > 0);
    }
}
