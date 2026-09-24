/*!
 * SOURCE OF TRUTH KEYWORDS: SileroVad, Silero VAD v5, silero_vad.onnx, 512-sample frame, 64-sample context, h/c state tensor, speech probability, hysteresis
 * WHAT:  SileroVad: VoiceActivity on the bundled Silero VAD v5 ONNX model. Each 512-sample frame (32 ms at 16 kHz)
 *        is run with the previous frame's last 64 samples as context and the model's recurrent state, and the
 *        speech probability becomes a verdict with hysteresis.
 * WHY:   Silero v5 is stateful: the `state` tensor and the 64-sample context carry across frames, so they are reset
 *        only in `reset` (once per take), never between frames (05 A11), and every frame must be exactly 512 samples
 *        (the capture worker buffers the remainder). The model's inputs are `input` [1, 576] f32, `state`
 *        [2, 1, 128] f32 and `sr` (int64 scalar); outputs are `output` [1, 1] (probability) and `stateN`; the
 *        signature is checked at load so a swapped model file fails loudly instead of misclassifying. Hysteresis
 *        (speech from 0.5, silence below 0.35) is the upstream default and stops a verdict flickering on a word's
 *        soft edge. The session is single-threaded: one frame takes well under a millisecond, and the capture
 *        worker must never compete with ASR for cores (05 A9). Buffers are allocated once, at load.
 * WHERE: Built by the registry engine entry `silero-vad-v5` (registry/engines.rs) from the bundled model path;
 *        owned as `Box<dyn VoiceActivity>` by the capture worker's segmenter (pipeline/capture/segmenter.rs).
 */

use std::path::Path;

use ort::{session::Session, value::TensorRef};

use crate::{
    adapters::onnx::{SessionThreads, onnx_failure, open_session},
    ports::VoiceActivity,
    types::{
        AppError, AppPaths, PIPELINE_SAMPLE_RATE_HZ, PortError, PortResult, VadCaps, VadEvent,
    },
};

/// Samples per frame the model takes at 16 kHz.
const FRAME: usize = 512;

/// Samples of the previous frame prepended to each frame.
const CONTEXT: usize = 64;

/// Shape of the recurrent state: [2, batch, 128].
const STATE_SHAPE: [usize; 3] = [2, 1, 128];
const STATE_LEN: usize = STATE_SHAPE[0] * STATE_SHAPE[1] * STATE_SHAPE[2];

/// The `sr` input: the model's sample rate as an int64 scalar.
const SAMPLE_RATE: [i64; 1] = [PIPELINE_SAMPLE_RATE_HZ as i64];

/// Probability at or above which a frame starts (or continues) speech.
const SPEECH_ON: f32 = 0.5;

/// Probability below which speech ends; between the two, the previous verdict holds.
const SPEECH_OFF: f32 = 0.35;

const INPUTS: [&str; 3] = ["input", "state", "sr"];
const OUTPUTS: [&str; 2] = ["output", "stateN"];

/// Silero VAD v5 on ONNX Runtime.
pub struct SileroVad {
    session: Session,
    /// CONTEXT samples of the previous frame, then the current frame.
    input: Vec<f32>,
    state: Vec<f32>,
    speaking: bool,
}

impl SileroVad {
    /// What this detector declares: 32 ms frames (512 samples at 16 kHz).
    pub const CAPS: VadCaps = VadCaps { frame_ms: 32 };

    /// Loads the model at `model` on the bundled ONNX Runtime; fails with `Internal` if it is missing or is not
    /// Silero v5.
    pub fn load(paths: &AppPaths, model: &Path) -> PortResult<Self> {
        if !model.is_file() {
            return Err(PortError::new(AppError::Internal).with_detail(format!(
                "the bundled VAD model is missing at {}",
                model.display()
            )));
        }
        let session = open_session(paths, model, SessionThreads::SINGLE)?;
        check_signature(&session)?;
        Ok(Self {
            session,
            input: vec![0.0; CONTEXT + FRAME],
            state: vec![0.0; STATE_LEN],
            speaking: false,
        })
    }

    /// Runs one frame and returns the model's speech probability, carrying state and context forward.
    fn probability(&mut self, frame: &[f32]) -> PortResult<f32> {
        self.input[CONTEXT..].copy_from_slice(frame);
        let input = TensorRef::from_array_view(([1, CONTEXT + FRAME], &self.input[..]))
            .map_err(|error| onnx_failure("wrap the VAD input", &error))?;
        let state = TensorRef::from_array_view((STATE_SHAPE, &self.state[..]))
            .map_err(|error| onnx_failure("wrap the VAD state", &error))?;
        let rate = TensorRef::from_array_view(((), &SAMPLE_RATE[..]))
            .map_err(|error| onnx_failure("wrap the VAD sample rate", &error))?;
        let outputs = self
            .session
            .run(ort::inputs!["input" => input, "state" => state, "sr" => rate])
            .map_err(|error| onnx_failure("run the VAD", &error))?;

        let probability = outputs
            .get(OUTPUTS[0])
            .ok_or_else(|| onnx_failure("read the VAD output", &"no `output` tensor"))?
            .try_extract_tensor::<f32>()
            .map_err(|error| onnx_failure("read the VAD output", &error))?
            .1
            .first()
            .copied()
            .ok_or_else(|| onnx_failure("read the VAD output", &"empty `output` tensor"))?;
        let next_state = outputs
            .get(OUTPUTS[1])
            .ok_or_else(|| onnx_failure("read the VAD state", &"no `stateN` tensor"))?
            .try_extract_tensor::<f32>()
            .map_err(|error| onnx_failure("read the VAD state", &error))?
            .1;
        if next_state.len() != STATE_LEN {
            return Err(onnx_failure(
                "read the VAD state",
                &format!("{} values, expected {STATE_LEN}", next_state.len()),
            ));
        }
        self.state.copy_from_slice(next_state);
        self.input.copy_within(FRAME.., 0);
        Ok(probability)
    }
}

impl VoiceActivity for SileroVad {
    fn caps(&self) -> VadCaps {
        Self::CAPS
    }

    fn reset(&mut self) -> PortResult<()> {
        self.input.fill(0.0);
        self.state.fill(0.0);
        self.speaking = false;
        Ok(())
    }

    fn push(&mut self, frame: &[f32]) -> PortResult<VadEvent> {
        if frame.len() != FRAME {
            return Err(PortError::new(AppError::Internal).with_detail(format!(
                "Silero VAD: frame of {} samples, expected {FRAME}",
                frame.len()
            )));
        }
        let probability = self.probability(frame)?;
        self.speaking = if self.speaking {
            probability >= SPEECH_OFF
        } else {
            probability >= SPEECH_ON
        };
        Ok(if self.speaking {
            VadEvent::Speech
        } else {
            VadEvent::Silence
        })
    }
}

/// Fails unless the session has exactly Silero v5's input and output names.
fn check_signature(session: &Session) -> PortResult<()> {
    let inputs: Vec<&str> = session.inputs().iter().map(|input| input.name()).collect();
    let outputs: Vec<&str> = session
        .outputs()
        .iter()
        .map(|output| output.name())
        .collect();
    if inputs == INPUTS && outputs == OUTPUTS {
        Ok(())
    } else {
        Err(PortError::new(AppError::Internal).with_detail(format!(
            "the VAD model is not Silero v5 (inputs {inputs:?}, outputs {outputs:?})"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;
    use crate::types::testing::{TempDir, source_resource_paths};

    fn load() -> (SileroVad, TempDir) {
        let data = TempDir::new("silero");
        let paths = source_resource_paths(data.path());
        let model = paths.bundled_models_dir().join("silero_vad.onnx");
        (SileroVad::load(&paths, &model).unwrap(), data)
    }

    #[test]
    fn silence_and_a_steady_hum_are_not_speech() {
        let (mut vad, _data) = load();
        vad.reset().unwrap();
        assert_eq!(vad.caps().frame_samples(), FRAME);
        for _ in 0..30 {
            assert_eq!(vad.push(&[0.0; FRAME]).unwrap(), VadEvent::Silence);
        }
        let hum: Vec<f32> = (0..FRAME * 30)
            .map(|index| 0.05 * (TAU * 50.0 * index as f32 / 16_000.0).sin())
            .collect();
        for frame in hum.as_chunks::<FRAME>().0 {
            assert_eq!(vad.push(frame).unwrap(), VadEvent::Silence);
        }
    }

    /// Speech from tests/fixtures (Windows text-to-speech, 16 kHz mono) framed by a second of silence.
    #[test]
    fn recorded_speech_is_detected_between_silences() {
        let (mut vad, _data) = load();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/speech-en-16k.wav");
        let reader = hound::WavReader::open(fixture).unwrap();
        assert_eq!(reader.spec().sample_rate, PIPELINE_SAMPLE_RATE_HZ);
        let speech: Vec<f32> = reader
            .into_samples::<i16>()
            .map(|sample| f32::from(sample.unwrap()) / f32::from(i16::MAX))
            .collect();
        let mut audio = vec![0.0; 16_000];
        audio.extend_from_slice(&speech);
        audio.extend(std::iter::repeat_n(0.0, 16_000));

        vad.reset().unwrap();
        let verdicts: Vec<VadEvent> = audio
            .as_chunks::<FRAME>()
            .0
            .iter()
            .map(|frame| vad.push(frame).unwrap())
            .collect();
        let lead = 16_000 / FRAME;
        assert!(
            verdicts[..lead]
                .iter()
                .all(|verdict| *verdict == VadEvent::Silence)
        );
        let spoken = &verdicts[lead..lead + speech.len() / FRAME];
        let speech_frames = spoken
            .iter()
            .filter(|verdict| **verdict == VadEvent::Speech)
            .count();
        assert!(
            speech_frames * 2 > spoken.len(),
            "{speech_frames} of {} frames detected as speech",
            spoken.len()
        );
        assert_eq!(verdicts.last(), Some(&VadEvent::Silence));
    }

    #[test]
    fn probabilities_are_deterministic_after_reset() {
        let (mut vad, _data) = load();
        let noise: Vec<f32> = (0..FRAME * 8)
            .map(|index| ((index * 7919 % 1000) as f32 / 1000.0 - 0.5) * 0.6)
            .collect();
        let run = |vad: &mut SileroVad| -> Vec<f32> {
            vad.reset().unwrap();
            noise
                .as_chunks::<FRAME>()
                .0
                .iter()
                .map(|frame| vad.probability(frame).unwrap())
                .collect()
        };
        let first = run(&mut vad);
        let second = run(&mut vad);
        assert_eq!(first, second, "reset clears state and context");
        assert!(
            first
                .iter()
                .all(|probability| (0.0..=1.0).contains(probability))
        );
    }

    #[test]
    fn frames_of_any_other_length_are_refused() {
        let (mut vad, _data) = load();
        assert_eq!(
            vad.push(&[0.0; FRAME - 1])
                .err()
                .map(PortError::into_app_error),
            Some(AppError::Internal)
        );
    }

    #[test]
    fn a_missing_model_is_reported() {
        let data = TempDir::new("silero-missing");
        let paths = source_resource_paths(data.path());
        let error = SileroVad::load(&paths, &data.join("silero_vad.onnx")).err();
        assert!(
            error
                .and_then(|error| error.detail().map(str::to_owned))
                .is_some()
        );
    }
}
