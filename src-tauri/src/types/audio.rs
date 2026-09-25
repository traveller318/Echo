/*!
 * SOURCE OF TRUTH KEYWORDS: AudioDevice, AudioTransport, EndpointChange, CaptureFormat, CaptureEvent, VadEvent, PIPELINE_SAMPLE_RATE_HZ, SegmentPolicy, SpeechSegment, CaptureSummary, MicCheck, AudioTestLevelInput
 * WHAT:  The audio data shapes: an input device the user can pick (AudioDevice) and how it is connected
 *        (AudioTransport), a raw device hot-plug notice (EndpointChange), the sample format an open
 *        capture stream delivers (CaptureFormat), what can happen to a stream besides samples (CaptureEvent), the
 *        per-frame voice activity verdict (VadEvent), the one sample rate the pipeline runs at, how a take is cut
 *        into segments (SegmentPolicy) and what the capture worker produces (SpeechSegment, CaptureSummary), plus
 *        the microphone check the UI asks for (AudioTestLevelInput → MicCheck).
 * WHY:   Capture adapters deliver whatever the device runs at (44.1/48 kHz, stereo); the pipeline's capture worker
 *        downmixes and resamples once to 16 kHz mono f32, which ASR, VAD and the WAV journal all consume (05 A2).
 *        So CaptureFormat describes the device side and PIPELINE_SAMPLE_RATE_HZ the pipeline side. Device loss is
 *        its own event because the session finalizes what was captured instead of discarding it (02 §5, 05 W12).
 *        VadEvent is a plain verdict: segmentation rules (600 ms pause, 20 s max, 02 §6.1) are pipeline policy, so
 *        they live in SegmentPolicy, which the pipeline narrows to the engine's `max_segment_s` caps. Segments carry
 *        their index because ASR may finish them out of order and text is joined by index (02 §6.1).
 * WHERE: AudioCapture and VoiceActivity ports (ports/audio.rs, ports/vad.rs); the capture worker in
 *        pipeline/capture produces SpeechSegment and CaptureSummary for the session actor and ASR worker;
 *        AudioDevice, AudioTestLevelInput and MicCheck cross IPC through `audio_list_devices` / `audio_test_level`.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{AudioDeviceId, PortError};

/// Sample rate, in Hz, of every stream after capture: ASR, VAD and the WAV journal all take 16 kHz mono f32.
pub const PIPELINE_SAMPLE_RATE_HZ: u32 = 16_000;

/// Milliseconds of 16 kHz mono audio in `samples` samples.
pub const fn samples_to_ms(samples: u64) -> u64 {
    samples * 1000 / PIPELINE_SAMPLE_RATE_HZ as u64
}

/// An audio input device the user can pick in Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AudioDevice {
    pub id: AudioDeviceId,
    /// Name as Windows shows it, e.g. `Microphone (USB Audio)`.
    pub name: String,
    /// Windows currently uses this device as the default input.
    pub is_default: bool,
    /// How the device is connected.
    pub transport: AudioTransport,
}

/**
 * SOURCE OF TRUTH KEYWORDS: AudioTransport, microphone connection, Bluetooth microphone, USB microphone, built-in microphone, virtual audio device
 * WHAT:  How an input device reaches the PC.
 * WHY:   A Bluetooth headset switches profile when its microphone opens and cuts the first 0.5–2 s (05 W11), so the
 *        pipeline must know the transport of the device a take opened to warn once; the Settings picker and
 *        onboarding can show it too. Unknown connections are `Other`, never guessed.
 * WHERE: AudioDevice.transport (audio_list_devices), CaptureStream::transport (the device a take opened);
 *        decided by the capture adapter.
 */
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AudioTransport {
    /// The PC's own audio hardware (integrated chipset or an internal card).
    BuiltIn,
    Usb,
    Bluetooth,
    /// Software routing (a virtual cable, a meeting app's device).
    Virtual,
    #[default]
    Other,
}

/**
 * SOURCE OF TRUTH KEYWORDS: EndpointChange, device hot-plug, microphone plugged, microphone unplugged, default input changed, device watch notice
 * WHAT:  One raw notice from Windows that the audio endpoints changed: a device appeared, went away, changed state
 *        (enabled, disabled, unplugged) or became the default input.
 * WHY:   Windows sends these in bursts (one plug is several notices, some about speakers), so the port only reports
 *        them and the pipeline debounces, re-lists the microphones and tells the UI only when the list really
 *        changed (pipeline/audio_devices.rs). The kind is kept for the log and for later rules (return to a pinned
 *        device); no device id travels here because the pipeline always re-lists.
 * WHERE: Emitted by the capture adapter's device watch (AudioCapture::watch_devices); consumed by
 *        pipeline/audio_devices.rs.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointChange {
    Added,
    Removed,
    StateChanged,
    DefaultInputChanged,
}

/// The samples an open capture stream hands its sink: interleaved f32 in [-1, 1] at the device's own rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaptureFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// Something that happened to an open capture stream outside the sample flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureEvent {
    /// The device disappeared (USB unplug, dock); what was captured so far is still valid.
    DeviceLost,
    /// The stream (or the worker processing it) failed and delivers no more samples.
    Failed(PortError),
}

/// Voice activity verdict for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VadEvent {
    Speech,
    Silence,
}

/**
 * SOURCE OF TRUTH KEYWORDS: SegmentPolicy, segmentation rules, 600 ms pause, 20 s max segment, pre-roll, min speech
 * WHAT:  How the capture worker cuts a take into segments that are transcribed while the user is still speaking.
 * WHY:   Cuts sit in silence so words are never split (02 §6.1, 05 A3). A cut forced by `max_segment_ms` goes into
 *        the latest pause in the second half of the segment when there is one. `pre_roll_ms` keeps a little
 *        silence before speech so the detector's onset latency never clips the first sound; `min_speech_ms`
 *        keeps clicks and breaths away from ASR, which would hallucinate a word from them (05 A4). The values are
 *        pipeline policy, not settings; `within_engine_limit` keeps a segment inside what the engine accepts.
 * WHERE: Built by the session actor from DEFAULT and the ASR caps; read by pipeline/capture/segmenter.rs.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentPolicy {
    /// Silence after speech that closes a segment, in ms.
    pub min_pause_ms: u32,
    /// Longest segment, in ms.
    pub max_segment_ms: u32,
    /// Silence kept in front of the first speech of a segment, in ms.
    pub pre_roll_ms: u32,
    /// A segment with less speech than this is not transcribed, in ms.
    pub min_speech_ms: u32,
}

impl SegmentPolicy {
    /// The values of 02 §6.1.
    pub const DEFAULT: Self = Self {
        min_pause_ms: 600,
        max_segment_ms: 20_000,
        pre_roll_ms: 200,
        min_speech_ms: 100,
    };

    /// This policy with segments no longer than an engine accepts in one call (`AsrCaps.max_segment_s`).
    #[must_use]
    pub const fn within_engine_limit(self, max_segment_s: u32) -> Self {
        let limit = max_segment_s.saturating_mul(1000);
        Self {
            max_segment_ms: if limit < self.max_segment_ms {
                limit
            } else {
                self.max_segment_ms
            },
            ..self
        }
    }
}

/// One stretch of a take that ends in a pause (or at the length limit), ready for ASR.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechSegment {
    /// Position in the take, from 0; the session joins text in this order (02 §6.1).
    pub index: u32,
    /// Where the segment starts in the take, in ms.
    pub start_ms: u64,
    /// How much of it the detector called speech, in ms.
    pub speech_ms: u32,
    /// 16 kHz mono samples, exactly as written to the journal.
    pub samples: Vec<f32>,
}

impl SpeechSegment {
    pub fn duration_ms(&self) -> u64 {
        samples_to_ms(self.samples.len() as u64)
    }
}

/// What the capture worker measured over a whole take.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CaptureSummary {
    /// Audio captured at 16 kHz (the journal's length), in ms; stored as `transcripts.duration_ms`.
    pub duration_ms: u64,
    /// Audio the detector called speech, in ms; stored as `transcripts.speech_ms` (05 A4 uses it for `empty`).
    pub speech_ms: u64,
    /// Segments handed to ASR.
    pub segments: u32,
    /// Loudest RMS over any level window, 0 to 1.
    pub peak_rms: f32,
    /// RMS over the whole take, 0 to 1.
    pub mean_rms: f32,
    /// Device audio lost because the worker fell behind the microphone, in ms (0 unless the machine stalled).
    pub dropped_ms: u64,
}

/// How a microphone check sounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MicVerdict {
    /// Digital silence: the device delivers nothing (muted, or blocked by Windows privacy, 05 W13).
    NoSignal,
    /// Some sound, but too quiet for reliable recognition.
    TooQuiet,
    Good,
    /// So loud that speech clips.
    TooLoud,
}

/// The result of `audio_test_level`: levels over the test window and what they mean.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct MicCheck {
    /// Loudest RMS over any level window, 0 to 1.
    pub peak_rms: f32,
    /// RMS over the whole window, 0 to 1.
    pub mean_rms: f32,
    pub verdict: MicVerdict,
}

/**
 * SOURCE OF TRUTH KEYWORDS: AudioTestLevelInput, audio_test_level input, microphone check window, garde schema
 * WHAT:  The input of `audio_test_level`: which device to listen to (None = the Windows default) and for how long.
 * WHY:   Settings tests a device before it is saved and onboarding tests the default, so the device is an input,
 *        not the stored setting. The factory enforces the window bounds and the device id shape before the mic
 *        opens (02 §4.1); whether the device is present is the adapter's answer. The window starts at 1 s because a
 *        device's first half second can be driver warm-up silence (05 W34).
 * WHERE: ipc/commands/audio.rs; built in the UI through the generated bindings.
 */
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, garde::Validate)]
pub struct AudioTestLevelInput {
    #[garde(custom(well_formed_device))]
    pub device: Option<AudioDeviceId>,
    /// How long to listen, in ms.
    #[garde(range(min = Self::MIN_WINDOW_MS, max = Self::MAX_WINDOW_MS))]
    pub window_ms: u32,
}

impl AudioTestLevelInput {
    /// At least 1 s: microphone arrays deliver about 0.5 s of digital silence while their driver warms up
    /// (05 W34), so a shorter window could hear nothing from a working microphone.
    pub const MIN_WINDOW_MS: u32 = 1_000;
    pub const MAX_WINDOW_MS: u32 = 5_000;
}

/// garde rule for an optional device id: None means the default device.
fn well_formed_device(device: &Option<AudioDeviceId>, (): &()) -> garde::Result {
    match device {
        Some(id) if !id.is_well_formed() => Err(garde::Error::new("Not an audio device id.")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use garde::Validate;
    use serde_json::json;

    use super::*;

    #[test]
    fn audio_device_serializes_with_a_plain_string_id() {
        let device = AudioDevice {
            id: AudioDeviceId::from_static("usb-mic"),
            name: String::from("Microphone (USB Audio)"),
            is_default: true,
            transport: AudioTransport::BuiltIn,
        };
        assert_eq!(
            serde_json::to_value(&device).unwrap(),
            json!({
                "id": "usb-mic",
                "name": "Microphone (USB Audio)",
                "is_default": true,
                "transport": "built_in"
            })
        );
    }

    #[test]
    fn segment_policy_follows_the_engine_limit() {
        assert_eq!(
            SegmentPolicy::DEFAULT
                .within_engine_limit(10)
                .max_segment_ms,
            10_000
        );
        assert_eq!(
            SegmentPolicy::DEFAULT.within_engine_limit(30),
            SegmentPolicy::DEFAULT
        );
        assert_eq!(samples_to_ms(16_000), 1_000);
        let segment = SpeechSegment {
            index: 0,
            start_ms: 0,
            speech_ms: 0,
            samples: vec![0.0; 8_000],
        };
        assert_eq!(segment.duration_ms(), 500);
    }

    #[test]
    fn mic_test_input_checks_the_window_and_the_device() {
        let input = |device: Option<&str>, window_ms| AudioTestLevelInput {
            device: device.map(|id| AudioDeviceId::from(id.to_owned())),
            window_ms,
        };
        assert!(input(None, 1_000).validate().is_ok());
        assert!(input(Some("wasapi:mic"), 1_000).validate().is_ok());
        assert!(input(None, 999).validate().is_err());
        assert!(input(None, 5_001).validate().is_err());
        assert!(input(Some(""), 1_000).validate().is_err());
        assert_eq!(
            serde_json::to_value(MicCheck {
                peak_rms: 0.5,
                mean_rms: 0.25,
                verdict: MicVerdict::NoSignal,
            })
            .unwrap(),
            json!({ "peak_rms": 0.5, "mean_rms": 0.25, "verdict": "no_signal" })
        );
    }
}
