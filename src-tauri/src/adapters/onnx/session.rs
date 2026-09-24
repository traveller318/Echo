/*!
 * SOURCE OF TRUTH KEYWORDS: open_session, ONNX session, SessionThreads, intra-op threads, inter-op threads, onnx_failure, ort error mapping
 * WHAT:  `open_session` makes sure the bundled runtime is loaded, then builds an ONNX Runtime CPU session for a model
 *        file with the given thread counts and full graph optimization; `onnx_failure` turns an `ort` error into
 *        a PortError with the cause as log detail.
 * WHY:   Every ONNX adapter (Silero VAD, Parakeet, a future engine) must start from the bundled runtime (05 A5) and
 *        must choose its own threads: small models run single-threaded so they never compete with ASR, and ASR
 *        takes cores − 1 (05 A9). Keeping the builder here means an adapter states only its model and threads, and
 *        the GPU (DirectML) path joins in one place (step 22). An `ort` error text can name files but never user
 *        audio or text, so it is safe as log detail; the user-facing code is `Internal` unless the caller knows
 *        better (a missing model is the caller's `ModelMissing`).
 * WHERE: adapters/vad/silero.rs; adapters/asr (next step).
 */

use std::path::Path;

use ort::session::{Session, builder::GraphOptimizationLevel};

use super::runtime::ensure_runtime;
use crate::types::{AppError, AppPaths, PortError, PortResult};

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
}

/// Opens `model` on the CPU with `threads`, loading the bundled ONNX Runtime first if needed.
pub fn open_session(
    paths: &AppPaths,
    model: &Path,
    threads: SessionThreads,
) -> PortResult<Session> {
    ensure_runtime(paths)?;
    Session::builder()
        .map_err(|error| onnx_failure("create a session builder", &error))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|error| onnx_failure("set the optimization level", &error))?
        .with_intra_threads(threads.intra_op)
        .map_err(|error| onnx_failure("set intra-op threads", &error))?
        .with_inter_threads(threads.inter_op)
        .map_err(|error| onnx_failure("set inter-op threads", &error))?
        .commit_from_file(model)
        .map_err(|error| onnx_failure("load the model", &error))
}

/// An ONNX Runtime failure while trying to `action`, with the runtime's message as log detail.
pub fn onnx_failure(action: &str, error: &dyn std::fmt::Display) -> PortError {
    PortError::new(AppError::Internal)
        .with_detail(format!("ONNX Runtime could not {action}: {error}"))
}
