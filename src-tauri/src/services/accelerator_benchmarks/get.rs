/*!
 * SOURCE OF TRUTH KEYWORDS: get accelerator benchmark, remembered measurement, read accelerator_benchmarks, unreadable benchmark row
 * WHAT:  `one`: the AcceleratorBenchmark remembered for `engine` on the GPU with fingerprint `gpu_key`, if any.
 * WHY:   A row whose JSON this build cannot read (a newer build, damage) is logged and treated as absent, so the
 *        engine is measured again instead of the load failing.
 * WHERE: pipeline/asr/accelerator.rs before measuring; service tests.
 */

use rusqlite::{OptionalExtension, params};

use crate::{
    services::db::Db,
    types::{AcceleratorBenchmark, EngineId, PortResult},
};

pub fn one(db: &Db, engine: &EngineId, gpu_key: &str) -> PortResult<Option<AcceleratorBenchmark>> {
    let json: Option<String> = db.read(|connection| {
        connection
            .prepare_cached(
                "SELECT benchmark_json FROM accelerator_benchmarks WHERE engine_id = ?1 AND gpu_key = ?2",
            )?
            .query_row(params![engine.as_str(), gpu_key], |row| row.get(0))
            .optional()
    })?;
    Ok(json.and_then(|json| match serde_json::from_str(&json) {
        Ok(benchmark) => Some(benchmark),
        Err(error) => {
            tracing::warn!(engine = %engine, %error, "remembered accelerator benchmark is unreadable; measuring again");
            None
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_remembered_at_first_and_unreadable_rows_count_as_absent() {
        let db = Db::open_in_memory().unwrap();
        let engine = EngineId::from_static("engine-a");
        assert_eq!(one(&db, &engine, "8086:1234:1.0").unwrap(), None);
        db.write(|connection| {
            connection.execute(
                "INSERT INTO accelerator_benchmarks VALUES ('engine-a', 'gpu', 'not json', 1)",
                [],
            )
        })
        .unwrap();
        assert_eq!(one(&db, &engine, "gpu").unwrap(), None);
    }
}
