/*!
 * SOURCE OF TRUTH KEYWORDS: VoiceActivity, VAD, voice activity detection, speech frame, silence frame, reset per take, Silero
 * WHAT:  VoiceActivity classifies fixed-size frames of 16 kHz mono audio as speech or silence.
 * WHY:   Detectors are stateful across frames (Silero keeps h/c tensors and a 64-sample context, 05 A11), so the
 *        methods take `&mut self` and the capture worker owns its detector as `Box<dyn VoiceActivity>` instead of a
 *        shared `Arc`. State is reset once per take, never mid-take. The frame length comes from
 *        `VadCaps.frame_ms`; the capture worker buffers the remainder so every call gets an exact frame.
 * WHERE: Implemented by adapters/vad/silero.rs (SileroVad) and ports/fakes; owned by pipeline/capture/segmenter.rs, which
 *        turns the verdicts into segment boundaries.
 */

use crate::types::{PortResult, VadCaps, VadEvent};

/// A voice activity detector for one audio stream at a time.
pub trait VoiceActivity: Send {
    fn caps(&self) -> VadCaps;

    /// Clears all detector state. Called at the start of every take.
    fn reset(&mut self) -> PortResult<()>;

    /// Classifies one frame of exactly `caps().frame_samples()` samples at 16 kHz mono; a frame of another length
    /// fails with `Internal`.
    fn push(&mut self, frame: &[f32]) -> PortResult<VadEvent>;
}
