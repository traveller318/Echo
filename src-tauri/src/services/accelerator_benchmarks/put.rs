/*!
 * SOURCE OF TRUTH KEYWORDS: put accelerator benchmark, remember measurement, upsert accelerator_benchmarks
 * WHAT:  `put`: remembers `benchmark` for `engine` on the GPU with fingerprint `gpu_key`, replacing an older one.
 * WHY:   One row per engine and GPU via upsert, so a re-measurement is a single statement and never leaves two
 *        answers for one key.
 * WHERE: pipeline/asr/accelerator.rs after measuring; service tests.
 */

use rusqlite::params;

use crate::{
    services::db::{Db, storage},
    types::{AcceleratorBenchmark, EngineId, PortResult},
};

pub fn put(
    db: &Db,
    engine: &EngineId,
    gpu_key: &str,
    benchmark: &AcceleratorBenchmark,
) -> PortResult<()> {
    let json = serde_json::to_string(benchmark).map_err(storage)?;
    db.write(|connection| {
        connection
            .prepare_cached(
                "INSERT INTO accelerator_benchmarks (engine_id, gpu_key, benchmark_json, measured_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT (engine_id, gpu_key) DO UPDATE SET benchmark_json = excluded.benchmark_json, \
                 measured_at = excluded.measured_at",
            )?
            .execute(params![
                engine.as_str(),
                gpu_key,
                json,
                benchmark.measured_at.as_millis()
            ])
            .map(drop)
    })
}

#[cfg(test)]
mod tests {
    use super::super::get;
    use super::*;
    use crate::types::{Accelerator, AcceleratorTiming, UnixMs};

    fn benchmark(chosen: Accelerator, at: i64) -> AcceleratorBenchmark {
        AcceleratorBenchmark {
            chosen,
            timings: vec![AcceleratorTiming {
                accelerator: Accelerator::Cpu,
                load_ms: 900,
                warm_up_ms: 300,
                run_ms: 150,
            }],
            measured_at: UnixMs::from_millis(at),
        }
    }

    #[test]
    fn a_measurement_is_remembered_per_engine_and_gpu_and_replaced_by_the_next() {
        let db = Db::open_in_memory().unwrap();
        let engine = EngineId::from_static("engine-a");
        put(&db, &engine, "gpu-1", &benchmark(Accelerator::Cpu, 1)).unwrap();
        put(&db, &engine, "gpu-1", &benchmark(Accelerator::Gpu, 2)).unwrap();
        put(&db, &engine, "gpu-2", &benchmark(Accelerator::Cpu, 3)).unwrap();
        assert_eq!(
            get::one(&db, &engine, "gpu-1").unwrap(),
            Some(benchmark(Accelerator::Gpu, 2))
        );
        assert_eq!(
            get::one(&db, &engine, "gpu-2").unwrap(),
            Some(benchmark(Accelerator::Cpu, 3))
        );
        assert_eq!(
            get::one(&db, &EngineId::from_static("engine-b"), "gpu-1").unwrap(),
            None
        );
    }
}
