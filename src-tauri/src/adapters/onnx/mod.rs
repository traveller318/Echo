/*!
 * SOURCE OF TRUTH KEYWORDS: onnx adapters, ONNX Runtime shared, bundled runtime, ensure_runtime, open_session, open_session_on, DirectML, SessionThreads, onnx_failure, physical cores
 * WHAT:  What every ONNX-based adapter shares: loading the bundled ONNX Runtime once (runtime.rs), opening a
 *        session with chosen threads (session.rs) and the machine's physical core count those threads derive from
 *        (cpu.rs).
 * WHY:   ONNX Runtime is one per process and must come from the bundle (05 A5); engine adapters (VAD, ASR) only name
 *        their model and threads. `ensure_runtime` is public so an adapter can tell a broken bundle (`Internal`)
 *        apart from a broken model file (`ModelCorrupt`) before it opens sessions. Nothing outside adapters/ ever
 *        sees `ort`.
 * WHERE: Used by adapters/vad and adapters/asr.
 */

mod cpu;
mod runtime;
mod session;

pub use runtime::ensure_runtime;
pub use session::{SessionThreads, onnx_failure, open_session, open_session_on};
