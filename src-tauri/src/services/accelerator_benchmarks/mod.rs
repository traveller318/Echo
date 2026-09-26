/*!
 * SOURCE OF TRUTH KEYWORDS: accelerator_benchmarks service, remembered accelerator choice, GPU benchmark cache, get put clear benchmark
 * WHAT:  The `accelerator_benchmarks` table's verbs, one file each: get (the measurement remembered for an engine on a
 *        GPU), put (remember one, replacing the old) and clear (forget every one of an engine).
 * WHY:   `auto` measures once per GPU and driver (02 §8.1, 05 A6); the rule of when to measure, reuse or forget lives
 *        in pipeline/asr/accelerator.rs, this service only stores. Rows hold the AcceleratorBenchmark as JSON; a row
 *        this build cannot read counts as absent, so the engine is simply measured again.
 * WHERE: `use crate::services::accelerator_benchmarks` from pipeline/asr/accelerator.rs (get, put) and the
 *        `engine_remeasure` command (clear).
 */

pub mod clear;
pub mod get;
pub mod put;
