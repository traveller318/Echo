/*!
 * SOURCE OF TRUTH KEYWORDS: AudioDevice, CaptureFormat, CaptureEvent, VadEvent, PIPELINE_SAMPLE_RATE_HZ, 16 kHz mono, device lost, voice activity
 * WHAT:  The audio data shapes: an input device the user can pick (AudioDevice), the sample format an open
 *        capture stream delivers (CaptureFormat), what can happen to a stream besides samples (CaptureEvent), the
 *        per-frame voice activity verdict (VadEvent) and the one sample rate the pipeline runs at.
 * WHY:   Capture adapters deliver whatever the device runs at (44.1/48 kHz, stereo); the pipeline's capture worker
 *        downmixes and resamples once to 16 kHz mono f32, which ASR, VAD and the WAV journal all consume (05 A2).
 *        So CaptureFormat describes the device side and PIPELINE_SAMPLE_RATE_HZ the pipeline side. Device loss is
 *        its own event because the session finalizes what was captured instead of discarding it (02 §5, 05 W12).
 *        VadEvent is a plain verdict: segmentation rules (600 ms pause, 20 s max) are pipeline policy, not VAD's.
 * WHERE: AudioCapture and VoiceActivity ports (ports/audio.rs, ports/vad.rs); AudioDevice crosses IPC through
 *        `audio_list_devices`; the capture worker in pipeline/capture.rs consumes the rest.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{AudioDeviceId, PortError};

/// Sample rate, in Hz, of every stream after capture: ASR, VAD and the WAV journal all take 16 kHz mono f32.
pub const PIPELINE_SAMPLE_RATE_HZ: u32 = 16_000;

/// An audio input device the user can pick in Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AudioDevice {
    pub id: AudioDeviceId,
    /// Name as Windows shows it, e.g. `Microphone (USB Audio)`.
    pub name: String,
    /// Windows currently uses this device as the default input.
    pub is_default: bool,
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
    /// The stream failed for another reason and delivers no more samples.
    Failed(PortError),
}

/// Voice activity verdict for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VadEvent {
    Speech,
    Silence,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn audio_device_serializes_with_a_plain_string_id() {
        let device = AudioDevice {
            id: AudioDeviceId::from_static("usb-mic"),
            name: String::from("Microphone (USB Audio)"),
            is_default: true,
        };
        assert_eq!(
            serde_json::to_value(&device).unwrap(),
            json!({ "id": "usb-mic", "name": "Microphone (USB Audio)", "is_default": true })
        );
    }
}
