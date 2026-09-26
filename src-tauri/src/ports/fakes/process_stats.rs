/*!
 * SOURCE OF TRUTH KEYWORDS: FakeProcessStats, fake memory use, About memory test
 * WHAT:  FakeProcessStats: a ProcessStats that reports a fixed ProcessMemory, or fails.
 * WHY:   `app_about` shows memory when Windows answers and nothing when it cannot; both need a test without
 *        depending on the test process's real memory.
 * WHERE: system command tests; the command harness.
 */

use crate::{
    ports::ProcessStats,
    types::{AppError, ByteCount, PortError, PortResult, ProcessMemory},
};

/// Fixed memory figures, or none.
pub struct FakeProcessStats {
    memory: Option<ProcessMemory>,
}

impl FakeProcessStats {
    /// Reports `working_set` and `private_bytes` bytes.
    pub const fn using(working_set: u64, private_bytes: u64) -> Self {
        Self {
            memory: Some(ProcessMemory {
                working_set: ByteCount::new(working_set),
                private_bytes: ByteCount::new(private_bytes),
            }),
        }
    }

    /// Windows cannot report the memory.
    pub const fn unreadable() -> Self {
        Self { memory: None }
    }
}

impl ProcessStats for FakeProcessStats {
    fn memory(&self) -> PortResult<ProcessMemory> {
        self.memory.ok_or_else(|| {
            PortError::new(AppError::Internal).with_detail("the fake reports no memory")
        })
    }
}
