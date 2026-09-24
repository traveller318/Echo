/*!
 * SOURCE OF TRUTH KEYWORDS: onnx adapters, ONNX Runtime shared, bundled runtime, open_session, SessionThreads, onnx_failure
 * WHAT:  What every ONNX-based adapter shares: loading the bundled ONNX Runtime once (runtime.rs) and opening a
 *        session with chosen threads (session.rs).
 * WHY:   ONNX Runtime is one per process and must come from the bundle (05 A5); engine adapters (VAD, ASR) only name
 *        their model and threads. Nothing outside adapters/ ever sees `ort`.
 * WHERE: Used by adapters/vad and adapters/asr.
 */

mod runtime;
mod session;

pub use session::{SessionThreads, onnx_failure, open_session};
