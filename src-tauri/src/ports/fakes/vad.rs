/*!
 * SOURCE OF TRUTH KEYWORDS: FakeVoiceActivity, fake VAD, amplitude threshold, speech frames, silence frames, reset count
 * WHAT:  FakeVoiceActivity: a VoiceActivity that calls a frame speech when any sample's magnitude exceeds
 *        SPEECH_THRESHOLD, and counts resets and frames.
 * WHY:   A deterministic rule lets a pipeline test write speech as non-zero samples and silence as zeros. The
 *        detector is owned as a Box by the capture worker, so clones share one log: the test keeps a clone to
 *        read `resets()` after handing the other to the pipeline.
 * WHERE: pipeline capture and segmentation tests.
 */

use std::sync::{Arc, Mutex};

use super::lock;
use crate::{
    ports::VoiceActivity,
    types::{AppError, PortError, PortResult, VadCaps, VadEvent},
};

#[derive(Default)]
struct VadLog {
    resets: usize,
    frames: usize,
}

/// An amplitude-threshold voice detector.
#[derive(Clone)]
pub struct FakeVoiceActivity {
    caps: VadCaps,
    log: Arc<Mutex<VadLog>>,
}

impl FakeVoiceActivity {
    /// Magnitude above which a sample counts as speech.
    pub const SPEECH_THRESHOLD: f32 = 0.01;

    pub fn new(frame_ms: u32) -> Self {
        Self {
            caps: VadCaps { frame_ms },
            log: Arc::default(),
        }
    }

    pub fn resets(&self) -> usize {
        lock(&self.log).resets
    }

    pub fn frames(&self) -> usize {
        lock(&self.log).frames
    }
}

impl VoiceActivity for FakeVoiceActivity {
    fn caps(&self) -> VadCaps {
        self.caps
    }

    fn reset(&mut self) -> PortResult<()> {
        lock(&self.log).resets += 1;
        Ok(())
    }

    fn push(&mut self, frame: &[f32]) -> PortResult<VadEvent> {
        let expected = self.caps.frame_samples();
        if frame.len() != expected {
            return Err(PortError::new(AppError::Internal).with_detail(format!(
                "fake VAD: frame of {} samples, expected {expected}",
                frame.len()
            )));
        }
        lock(&self.log).frames += 1;
        let speech = frame
            .iter()
            .any(|sample| sample.abs() > Self::SPEECH_THRESHOLD);
        Ok(if speech {
            VadEvent::Speech
        } else {
            VadEvent::Silence
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_exact_frames_by_amplitude() {
        let probe = FakeVoiceActivity::new(32);
        let mut vad: Box<dyn VoiceActivity> = Box::new(probe.clone());
        vad.reset().unwrap();
        let samples = vad.caps().frame_samples();
        assert_eq!(vad.push(&vec![0.0; samples]).unwrap(), VadEvent::Silence);
        let mut speech = vec![0.0; samples];
        speech[100] = 0.4;
        assert_eq!(vad.push(&speech).unwrap(), VadEvent::Speech);
        assert!(vad.push(&[0.0; 10]).is_err());
        assert_eq!((probe.resets(), probe.frames()), (1, 2));
    }
}
