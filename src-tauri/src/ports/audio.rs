/*!
 * SOURCE OF TRUTH KEYWORDS: AudioCapture, CaptureStream, AudioSink, microphone capture, start capture, pause resume, stop capture, real-time callback
 * WHAT:  AudioCapture lists input devices and opens one; the open stream is a CaptureStream handle (pause, resume,
 *        stop) and its samples go to an AudioSink on the real-time audio thread.
 * WHY:   A handle per take instead of start/stop methods on the port means pausing a stream that was never opened
 *        cannot compile, and dropping the handle on any exit path (error, shutdown, panic unwinding) closes the
 *        microphone. The sink is pushed from the device callback, which must not block, lock or allocate
 *        (02 §6.1), so it is a `&mut self` trait moved onto that thread rather than a shared channel. Samples
 *        arrive in the device's own format (CaptureFormat); downmix and resample to 16 kHz happen once in the
 *        pipeline (05 A2), so a new backend (WASAPI exclusive, a file source) only converts to f32.
 * WHERE: Implemented by adapters/audio/cpal_wasapi.rs (CpalWasapiCapture) and ports/fakes; held by the pipeline
 *        as `Arc<dyn AudioCapture>`; the capture worker (pipeline/capture, RingSink) implements AudioSink.
 */

use std::sync::Arc;

use super::EventSink;
use crate::types::{
    AudioCaps, AudioDevice, AudioDeviceId, CaptureEvent, CaptureFormat, PortResult,
};

/// Receives captured samples on the real-time audio thread.
pub trait AudioSink: Send + 'static {
    /// Interleaved f32 samples in the stream's CaptureFormat. Must not block, lock or allocate.
    fn push(&mut self, samples: &[f32]);
}

/// An open input stream. Dropping it closes the device.
pub trait CaptureStream: Send {
    /// The format of every sample slice the sink receives.
    fn format(&self) -> CaptureFormat;

    /// Stops delivering samples while keeping the device open (the Esc countdown); audio during the pause is
    /// dropped. Pausing a paused stream is a no-op.
    fn pause(&mut self) -> PortResult<()>;

    /// Delivers samples again after `pause`. Resuming a running stream is a no-op.
    fn resume(&mut self) -> PortResult<()>;

    /// Closes the device after every sample already captured has reached the sink.
    fn stop(self: Box<Self>) -> PortResult<()>;
}

/// Microphone access.
pub trait AudioCapture: Send + Sync {
    fn caps(&self) -> AudioCaps;

    /// Input devices currently present.
    fn devices(&self) -> PortResult<Vec<AudioDevice>>;

    /**
     * SOURCE OF TRUTH KEYWORDS: AudioCapture::start, open microphone, pinned device, system default device
     * WHAT:  Opens `device` (None = the Windows default input) and starts pushing samples into `sink`; stream
     *        failures and device loss go to `events`. Returns the stream handle.
     * WHY:   One stream at a time: a second `start` while a stream is open fails with `Busy`. A pinned device that
     *        is no longer present fails with `NotFound { audio_device }` so the pipeline can decide to fall back
     *        to the default; a blocked microphone privacy consent fails with `PermissionDenied { microphone }`
     *        (05 W13); anything else is `AudioDevice`.
     * WHERE: The session actor's `RecordPressed` effect (after the transcripts row exists, 02 §7.3).
     */
    fn start(
        &self,
        device: Option<&AudioDeviceId>,
        sink: Box<dyn AudioSink>,
        events: Arc<dyn EventSink<CaptureEvent>>,
    ) -> PortResult<Box<dyn CaptureStream>>;
}
