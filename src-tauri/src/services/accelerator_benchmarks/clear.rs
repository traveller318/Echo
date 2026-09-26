/*!
 * SOURCE OF TRUTH KEYWORDS: clear accelerator benchmarks, forget measurement, remeasure accelerator, delete accelerator_benchmarks
 * WHAT:  `for_engine`: forgets every measurement remembered for `engine` (on any GPU); returns how many were removed.
 * WHY:   "Measure again" must forget the old GPUs and drivers too, or switching back to one would reuse a stale answer.
 * WHERE: `engine_remeasure` (ipc/commands/engine.rs); service tests.
 */

use rusqlite::params;

use crate::{
    services::db::Db,
    types::{EngineId, PortResult},
};

pub fn for_engine(db: &Db, engine: &EngineId) -> PortResult<usize> {
    db.write(|connection| {
        connection
            .prepare_cached("DELETE FROM accelerator_benchmarks WHERE engine_id = ?1")?
            .execute(params![engine.as_str()])
    })
}

#[cfg(test)]
mod tests {
    use super::super::{get, put};
    use super::*;
    use crate::types::{Accelerator, AcceleratorBenchmark, UnixMs};

    #[test]
    fn only_the_engine_s_measurements_are_forgotten() {
        let db = Db::open_in_memory().unwrap();
        let a = EngineId::from_static("engine-a");
        let b = EngineId::from_static("engine-b");
        let benchmark = AcceleratorBenchmark {
            chosen: Accelerator::Cpu,
            timings: Vec::new(),
            measured_at: UnixMs::from_millis(1),
        };
        for (engine, gpu) in [(&a, "gpu-1"), (&a, "gpu-2"), (&b, "gpu-1")] {
            put::put(&db, engine, gpu, &benchmark).unwrap();
        }
        assert_eq!(for_engine(&db, &a).unwrap(), 2);
        assert_eq!(get::one(&db, &a, "gpu-1").unwrap(), None);
        assert!(get::one(&db, &b, "gpu-1").unwrap().is_some());
        assert_eq!(for_engine(&db, &a).unwrap(), 0);
    }
}
