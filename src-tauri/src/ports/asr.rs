/*!
 * SOURCE OF TRUTH KEYWORDS: AsrEngine, speech recognition, transcribe, load model, warm up, unload, accelerator fallback, ComputeDevice, ASR port
 * WHAT:  AsrEngine turns a segment of 16 kHz mono audio into text, after loading a model from a folder onto an
 *        accelerator and warming it up.
 * WHY:   The core never names a model (00 constraint 4): Parakeet, Whisper or Moonshine are each one adapter with
 *        honest AsrCaps (02 §8.4). The engine is shared as `Arc<dyn AsrEngine>` so an engine switch can swap the
 *        `Arc` while a take finishes on the old one (02 §8.1); methods therefore take `&self` and adapters keep
 *        their session behind interior mutability. `load` returns the accelerator actually in use because a GPU
 *        session that fails to start falls back to CPU silently (05 A6). All calls block; the ASR worker runs
 *        them on its own OS thread (02 §6.1).
 * WHERE: Implemented by adapters/asr/parakeet_onnx.rs (ParakeetOnnx) and ports/fakes; built by the registry
 *        engines entry; owned, loaded and warmed by the ASR worker (pipeline/asr).
 */

use std::path::Path;

use crate::types::{Accelerator, AsrCaps, AsrOutput, ComputeDevice, Language, PortResult};

/// A local speech recognition engine.
pub trait AsrEngine: Send + Sync {
    fn caps(&self) -> AsrCaps;

    /// Loads the model files in `model_dir` onto `device` (whose accelerator is one of `caps().accelerators`),
    /// replacing any loaded session. Returns the accelerator in use: a GPU session that cannot start falls back to
    /// the CPU, logged, not failed (05 A6). Missing files fail with `ModelMissing`, unreadable ones with
    /// `ModelCorrupt`.
    fn load(&self, model_dir: &Path, device: &ComputeDevice) -> PortResult<Accelerator>;

    /// Runs one inference on silence so the first real take is as fast as the tenth (05 A8).
    fn warm_up(&self) -> PortResult<()>;

    /// Frees the loaded session and its memory. Unloading an unloaded engine is a no-op.
    fn unload(&self) -> PortResult<()>;

    /// Transcribes one segment (at most `caps().max_segment_s` seconds of 16 kHz mono f32). `language` None means
    /// auto-detect and is only passed when `caps().auto_language`. Fails with `Asr` when no model is loaded.
    fn transcribe(&self, audio: &[f32], language: Option<&Language>) -> PortResult<AsrOutput>;
}
