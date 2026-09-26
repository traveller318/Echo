/*!
 * SOURCE OF TRUTH KEYWORDS: open_session, open_session_on, ONNX session, DirectML session, SessionThreads, for_asr, intra-op threads, onnx_failure, ort error mapping
 * WHAT:  `open_session_on` makes sure the bundled runtime is loaded, then builds an ONNX Runtime session for a model
 *        file on a device (the CPU, or a GPU through the DirectML execution provider) with the given thread counts
 *        and full graph optimization; `open_session` is the CPU case; `onnx_failure` turns an `ort` error into a
 *        PortError with the cause as log detail.
 * WHY:   Every ONNX adapter (Silero VAD, Parakeet, a future engine) must start from the bundled runtime (05 A5) and
 *        must choose its own threads: small models run single-threaded so they never compete with ASR, and ASR
 *        takes physical cores − 1 (the performance cores on a hybrid CPU), at least 2 (05 A9, W35,
 *        `SessionThreads::for_asr`). Keeping the builder here means an
 *        adapter states only its model, threads and device. DirectML: the provider opens the GPU by its DXGI
 *        enumeration index (GpuAdapter.ordinal, adapters/gpu); it must fail loudly (`error_on_failure`) instead of
 *        `ort`'s default of silently running on the CPU, so the caller knows the GPU did not start and can say so
 *        (05 A6); ONNX Runtime requires memory patterns and parallel execution off with DirectML; operators DirectML
 *        lacks still run on the CPU provider inside the same session, with the given threads. An `ort` error text can name files but never user
 *        audio or text, so it is safe as log detail; the user-facing code is `Internal` unless the caller knows
 *        better (a missing model is the caller's `ModelMissing`).
 * WHERE: adapters/vad/silero.rs (SINGLE, CPU); adapters/asr/parakeet_onnx (for_asr; the encoder on the device the
 *        engine was loaded on).
 */

use std::path::Path;

use ort::{
    ep::DirectML,
    session::{Session, builder::GraphOptimizationLevel},
};

use super::{
    cpu::{CpuCores, cpu_cores},
    runtime::ensure_runtime,
};
use crate::types::{AppError, AppPaths, ComputeDevice, PortError, PortResult};

/// How many threads one ONNX session may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionThreads {
    /// Threads inside one operator (matrix multiplies).
    pub intra_op: usize,
    /// Operators run in parallel.
    pub inter_op: usize,
}

impl SessionThreads {
    /// One thread each: for small models that run on a busy worker thread (the VAD).
    pub const SINGLE: Self = Self {
        intra_op: 1,
        inter_op: 1,
    };

    /**
     * SOURCE OF TRUTH KEYWORDS: for_asr, ASR thread count, 05 A9 rule, hybrid CPU threads
     * WHAT:  The ASR session's threads on this machine; `for_asr_on` is the rule for given core counts: operators
     *        in sequence (inter-op 1) and an intra-op pool of physical cores − 1 (one core stays free for audio
     *        capture and the UI), or on a hybrid CPU of the performance cores only (the efficiency cores are then
     *        what stays free), never fewer than 2.
     * WHY:   05 A9, refined by the hybrid measurement in 05 W35: ONNX Runtime waits for its slowest thread, so a
     *        pool that spills onto efficiency cores runs slower than one sized to the fast cores.
     * WHERE: adapters/asr/parakeet_onnx (preprocessor and encoder sessions).
     */
    pub fn for_asr() -> Self {
        Self::for_asr_on(cpu_cores())
    }

    pub const fn for_asr_on(cores: CpuCores) -> Self {
        let pool = if cores.is_hybrid() {
            cores.performance
        } else {
            cores.physical.saturating_sub(1)
        };
        Self {
            intra_op: if pool < 2 { 2 } else { pool },
            inter_op: 1,
        }
    }
}

/// Opens `model` on the CPU with `threads`, loading the bundled ONNX Runtime first if needed.
pub fn open_session(
    paths: &AppPaths,
    model: &Path,
    threads: SessionThreads,
) -> PortResult<Session> {
    open_session_on(paths, model, threads, &ComputeDevice::Cpu)
}

/// Opens `model` on `device` with `threads`, loading the bundled ONNX Runtime first if needed. A GPU that cannot
/// start fails the call (the caller decides whether to fall back).
pub fn open_session_on(
    paths: &AppPaths,
    model: &Path,
    threads: SessionThreads,
    device: &ComputeDevice,
) -> PortResult<Session> {
    ensure_runtime(paths)?;
    let builder = Session::builder()
        .map_err(|error| onnx_failure("create a session builder", &error))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|error| onnx_failure("set the optimization level", &error))?
        .with_intra_threads(threads.intra_op)
        .map_err(|error| onnx_failure("set intra-op threads", &error))?
        .with_inter_threads(threads.inter_op)
        .map_err(|error| onnx_failure("set inter-op threads", &error))?;
    let mut builder = match device {
        ComputeDevice::Cpu => builder,
        ComputeDevice::Gpu(gpu) => {
            let device_id = i32::try_from(gpu.ordinal).map_err(|_| {
                PortError::new(AppError::Internal)
                    .with_detail(format!("GPU ordinal {} is out of range", gpu.ordinal))
            })?;
            builder
                .with_memory_pattern(false)
                .map_err(|error| onnx_failure("turn memory patterns off for DirectML", &error))?
                .with_parallel_execution(false)
                .map_err(|error| onnx_failure("run operators in sequence for DirectML", &error))?
                .with_execution_providers([DirectML::default()
                    .with_device_id(device_id)
                    .build()
                    .error_on_failure()])
                .map_err(|error| onnx_failure("start DirectML on the GPU", &error))?
        }
    };
    builder
        .commit_from_file(model)
        .map_err(|error| onnx_failure("load the model", &error))
}

/// An ONNX Runtime failure while trying to `action`, with the runtime's message as log detail.
pub fn onnx_failure(action: &str, error: &dyn std::fmt::Display) -> PortError {
    PortError::new(AppError::Internal)
        .with_detail(format!("ONNX Runtime could not {action}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asr_threads_leave_one_core_free_and_never_drop_below_two() {
        let uniform = |physical| CpuCores {
            physical,
            performance: physical,
        };
        assert_eq!(
            SessionThreads::for_asr_on(uniform(8)),
            SessionThreads {
                intra_op: 7,
                inter_op: 1
            }
        );
        assert_eq!(SessionThreads::for_asr_on(uniform(2)).intra_op, 2);
        assert_eq!(SessionThreads::for_asr_on(uniform(1)).intra_op, 2);
        assert!(SessionThreads::for_asr().intra_op >= 2);
    }

    #[test]
    fn hybrid_cpus_run_asr_on_their_performance_cores() {
        let hybrid = |physical, performance| CpuCores {
            physical,
            performance,
        };
        assert_eq!(SessionThreads::for_asr_on(hybrid(8, 4)).intra_op, 4);
        assert_eq!(SessionThreads::for_asr_on(hybrid(14, 6)).intra_op, 6);
        assert_eq!(SessionThreads::for_asr_on(hybrid(10, 1)).intra_op, 2);
    }
}
