-- SOURCE OF TRUTH KEYWORDS: migration 0002, accelerator_benchmarks table, remembered accelerator choice, GPU benchmark cache, per GPU and driver
-- WHAT:  The `accelerator_benchmarks` table: one remembered `auto` measurement per speech engine and GPU (its
--        fingerprint: vendor, device and driver version), as the AcceleratorBenchmark JSON (the chosen accelerator
--        and every timing), stamped with when it was measured.
-- WHY:   02 §8.1 / 05 A6: `auto` measures the GPU against the CPU once per GPU and driver and reuses the answer, so a
--        normal start loads one engine instead of two. The key is the engine plus the GPU fingerprint, so a new GPU
--        or a driver update misses and measures again; the JSON keeps the row readable when a later build adds a
--        timing (an NPU) without another migration. Rows are cache, not history: clearing them only costs one
--        measurement.
-- WHERE: Embedded by services/db.rs (`MIGRATIONS`); read and written only by services/accelerator_benchmarks.

CREATE TABLE accelerator_benchmarks (
  engine_id TEXT NOT NULL,
  gpu_key TEXT NOT NULL,
  benchmark_json TEXT NOT NULL,
  measured_at INTEGER NOT NULL,
  PRIMARY KEY (engine_id, gpu_key)
);
