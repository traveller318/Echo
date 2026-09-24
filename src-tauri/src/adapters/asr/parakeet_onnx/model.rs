/*!
 * SOURCE OF TRUTH KEYWORDS: ParakeetModel, Parakeet ONNX sessions, nemo128 preprocessor, encoder-model int8, decoder_joint, OnnxJoint, decoder state, model signature check
 * WHAT:  ParakeetModel: the three ONNX Runtime sessions of the Parakeet TDT export (mel preprocessor, encoder,
 *        prediction + joint network) plus the vocabulary; `open` loads and checks them, `transcribe` turns 16 kHz
 *        mono samples into text (preprocess → encode → TDT greedy decode → detokenize).
 * WHY:   Our own decode on `ort` instead of wrapping transcribe-rs (05 A10, decision log 2026-09-24): transcribe-rs
 *        0.3.11 opens Parakeet sessions with ONNX Runtime's default thread pools (05 A9 cannot be applied), decodes
 *        it as plain RNN-T (ignoring the TDT durations) and compiles every other engine it ships. The export's
 *        signature (input/output names, int32 token inputs, recurrent state shape, logit count) is checked at load,
 *        so a wrong or damaged file is `ModelCorrupt` at startup instead of garbage text in a take. The preprocessor's
 *        output value is handed to the encoder as is (no copy); encoder frames are transposed once to frame-major
 *        rows so each decoder step reads one contiguous slice. 250 ms of leading silence is prepended, as the
 *        reference Parakeet pipelines do: the encoder needs left context before the first word or clips its onset.
 *        Errors while loading are `ModelCorrupt { model_id }` (except a broken ONNX Runtime bundle, `Internal`);
 *        errors while transcribing are `Asr`. Details name tensors and files, never text or audio (02 §10).
 * WHERE: Owned by ParakeetOnnx (parakeet_onnx/mod.rs) behind its mutex; decode loop in tdt.rs, tokens in vocab.rs.
 */

use std::path::Path;

use ort::{
    session::{Session, SessionOutputs},
    value::{DynValue, TensorElementType, TensorRef},
};

use super::{
    ENCODER, JOINT, PREPROCESSOR, VOCAB,
    tdt::{self, DURATIONS, Joint},
    vocab::Vocabulary,
};
use crate::{
    adapters::onnx::{SessionThreads, ensure_runtime, open_session},
    types::{AppError, AppPaths, ModelId, PIPELINE_SAMPLE_RATE_HZ, PortError, PortResult},
};

/// Silence prepended to every segment, in samples (250 ms at 16 kHz).
const LEAD_SILENCE: usize = PIPELINE_SAMPLE_RATE_HZ as usize / 4;

const PREPROCESSOR_IO: Io = Io {
    inputs: &["waveforms", "waveforms_lens"],
    outputs: &["features", "features_lens"],
};
const ENCODER_IO: Io = Io {
    inputs: &["audio_signal", "length"],
    outputs: &["outputs", "encoded_lengths"],
};
const JOINT_IO: Io = Io {
    inputs: &[
        "encoder_outputs",
        "targets",
        "target_length",
        "input_states_1",
        "input_states_2",
    ],
    outputs: &["outputs", "output_states_1", "output_states_2"],
};

/// Tensor names one session must have.
struct Io {
    inputs: &'static [&'static str],
    outputs: &'static [&'static str],
}

/// The loaded Parakeet TDT export.
pub struct ParakeetModel {
    preprocessor: Session,
    encoder: Session,
    joint: Session,
    vocab: Vocabulary,
    /// Shape of each recurrent state tensor: [layers, batch 1, hidden].
    state_shape: [usize; 3],
}

impl ParakeetModel {
    /**
     * SOURCE OF TRUTH KEYWORDS: ParakeetModel::open, load Parakeet sessions, ModelCorrupt, signature check
     * WHAT:  Loads the bundled ONNX Runtime, the vocabulary and the three sessions from `dir`, and checks the export's
     *        signature; `heavy` threads go to the preprocessor and encoder, the joint network runs single-threaded.
     * WHY:   The joint network is tiny and runs once per decode step, where thread hand-offs cost more than they
     *        save; the encoder is where the cores pay off (05 A9). The caller has already checked every file exists.
     * WHERE: ParakeetOnnx::load.
     */
    pub fn open(
        paths: &AppPaths,
        dir: &Path,
        model_id: &ModelId,
        heavy: SessionThreads,
    ) -> PortResult<Self> {
        ensure_runtime(paths)?;
        let corrupt = |detail: String| {
            PortError::new(AppError::ModelCorrupt {
                model_id: model_id.clone(),
            })
            .with_detail(detail)
        };
        let vocab_text = std::fs::read_to_string(dir.join(VOCAB))
            .map_err(|error| corrupt(format!("{VOCAB} is unreadable: {error}")))?;
        let vocab = Vocabulary::parse(&vocab_text).map_err(corrupt)?;
        let open = |file: &str, threads: SessionThreads, io: &Io| -> PortResult<Session> {
            let session = open_session(paths, &dir.join(file), threads).map_err(|error| {
                corrupt(format!(
                    "{file}: {}",
                    error.detail().unwrap_or("does not load")
                ))
            })?;
            check_names(&session, io).map_err(|detail| corrupt(format!("{file}: {detail}")))?;
            Ok(session)
        };
        let preprocessor = open(PREPROCESSOR, heavy, &PREPROCESSOR_IO)?;
        let encoder = open(ENCODER, heavy, &ENCODER_IO)?;
        let joint = open(JOINT, SessionThreads::SINGLE, &JOINT_IO)?;
        let state_shape = check_joint(&joint, vocab.len())
            .map_err(|detail| corrupt(format!("{JOINT}: {detail}")))?;
        Ok(Self {
            preprocessor,
            encoder,
            joint,
            vocab,
            state_shape,
        })
    }

    /// Transcribes 16 kHz mono samples; empty audio is empty text.
    pub fn transcribe(&mut self, samples: &[f32]) -> PortResult<String> {
        if samples.is_empty() {
            return Ok(String::new());
        }
        let mut audio = vec![0.0; LEAD_SILENCE];
        audio.extend_from_slice(samples);
        let (frames, dim, count) = self.encode(&audio)?;
        let mut joint = OnnxJoint::new(&mut self.joint, &frames, dim, self.state_shape);
        let tokens = tdt::greedy_decode(&mut joint, count, self.vocab.len(), self.vocab.blank())?;
        Ok(self.vocab.decode(&tokens))
    }

    /// Runs the preprocessor and encoder; returns frame-major encoder rows, their width and the valid frame count.
    fn encode(&mut self, audio: &[f32]) -> PortResult<(Vec<f32>, usize, usize)> {
        let waveform = TensorRef::from_array_view(([1, audio.len()], audio))
            .map_err(|error| asr_failure("wrap the waveform", &error))?;
        let length = [i64::try_from(audio.len()).unwrap_or(i64::MAX)];
        let length = TensorRef::from_array_view(([1], &length[..]))
            .map_err(|error| asr_failure("wrap the waveform length", &error))?;
        let features = self
            .preprocessor
            .run(ort::inputs!["waveforms" => waveform, "waveforms_lens" => length])
            .map_err(|error| asr_failure("run the preprocessor", &error))?;
        let encoded = self
            .encoder
            .run(ort::inputs![
                "audio_signal" => output(&features, "features")?,
                "length" => output(&features, "features_lens")?,
            ])
            .map_err(|error| asr_failure("run the encoder", &error))?;

        let (shape, data) = output(&encoded, "outputs")?
            .try_extract_tensor::<f32>()
            .map_err(|error| asr_failure("read the encoder output", &error))?;
        let [batch, dim, frames] = shape[..] else {
            return Err(asr_failure(
                "read the encoder output",
                &format!("shape {shape:?}"),
            ));
        };
        let (batch, dim, frames) = (dims(batch)?, dims(dim)?, dims(frames)?);
        if batch != 1 || data.len() != dim * frames {
            return Err(asr_failure(
                "read the encoder output",
                &format!("shape {shape:?}"),
            ));
        }
        let valid = output(&encoded, "encoded_lengths")?
            .try_extract_tensor::<i64>()
            .map_err(|error| asr_failure("read the encoded length", &error))?
            .1
            .first()
            .map_or(frames, |valid| {
                usize::try_from(*valid).unwrap_or(0).min(frames)
            });
        // [dim, frames] → [frames, dim]: each decode step reads one contiguous row.
        let mut rows = Vec::with_capacity(data.len());
        for frame in 0..frames {
            rows.extend((0..dim).map(|channel| data[channel * frames + frame]));
        }
        Ok((rows, dim, valid))
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: OnnxJoint, prediction network state, joint network step, decoder_joint run
 * WHAT:  The Joint the TDT decode drives: one decoder_joint run per step for one encoder row and the last token,
 *        keeping the produced recurrent state as a candidate until the decode commits it.
 * WHY:   The state must advance only on a real token (tdt.rs); swapping two preallocated buffers makes commit free and
 *        keeps the loop allocation-free apart from the output copy.
 * WHERE: ParakeetModel::transcribe.
 */
struct OnnxJoint<'a> {
    session: &'a mut Session,
    rows: &'a [f32],
    dim: usize,
    state_shape: [usize; 3],
    state: [Vec<f32>; 2],
    candidate: [Vec<f32>; 2],
    logits: Vec<f32>,
}

impl<'a> OnnxJoint<'a> {
    fn new(session: &'a mut Session, rows: &'a [f32], dim: usize, state_shape: [usize; 3]) -> Self {
        let size = state_shape.iter().product();
        Self {
            session,
            rows,
            dim,
            state_shape,
            state: [vec![0.0; size], vec![0.0; size]],
            candidate: [vec![0.0; size], vec![0.0; size]],
            logits: Vec::new(),
        }
    }
}

impl Joint for OnnxJoint<'_> {
    fn logits(&mut self, frame: usize, last_token: usize) -> PortResult<&[f32]> {
        let row = self
            .rows
            .get(frame * self.dim..(frame + 1) * self.dim)
            .ok_or_else(|| asr_failure("read an encoder frame", &format!("frame {frame}")))?;
        let token = [i32::try_from(last_token)
            .map_err(|_| asr_failure("wrap the last token", &format!("id {last_token}")))?];
        let length = [1_i32];
        let wrap = |error: ort::Error| asr_failure("wrap a decoder input", &error);
        let outputs = self
            .session
            .run(ort::inputs![
                "encoder_outputs" => TensorRef::from_array_view(([1, self.dim, 1], row)).map_err(wrap)?,
                "targets" => TensorRef::from_array_view(([1, 1], &token[..])).map_err(wrap)?,
                "target_length" => TensorRef::from_array_view(([1], &length[..])).map_err(wrap)?,
                "input_states_1" => TensorRef::from_array_view((self.state_shape, &self.state[0][..])).map_err(wrap)?,
                "input_states_2" => TensorRef::from_array_view((self.state_shape, &self.state[1][..])).map_err(wrap)?,
            ])
            .map_err(|error| asr_failure("run the decoder", &error))?;
        let read = |name: &str| -> PortResult<&[f32]> {
            output(&outputs, name)?
                .try_extract_tensor::<f32>()
                .map(|(_, data)| data)
                .map_err(|error| asr_failure("read a decoder output", &error))
        };
        let logits = read("outputs")?;
        self.logits.clear();
        self.logits.extend_from_slice(logits);
        for (candidate, name) in self
            .candidate
            .iter_mut()
            .zip(["output_states_1", "output_states_2"])
        {
            let next = read(name)?;
            if next.len() != candidate.len() {
                return Err(asr_failure(
                    "read the decoder state",
                    &format!(
                        "{name} has {} values, expected {}",
                        next.len(),
                        candidate.len()
                    ),
                ));
            }
            candidate.copy_from_slice(next);
        }
        Ok(&self.logits)
    }

    fn commit(&mut self) {
        std::mem::swap(&mut self.state, &mut self.candidate);
    }
}

/// Output `name` of a finished run.
fn output<'o>(outputs: &'o SessionOutputs<'_>, name: &str) -> PortResult<&'o DynValue> {
    outputs
        .get(name)
        .ok_or_else(|| asr_failure("read the model output", &format!("no `{name}` tensor")))
}

/// A tensor dimension as a size; symbolic (-1) dimensions are not sizes.
fn dims(value: i64) -> PortResult<usize> {
    usize::try_from(value)
        .map_err(|_| asr_failure("read a tensor shape", &format!("dimension {value}")))
}

/// An inference failure while trying to `action`: `Asr` for the user, the cause for the log.
fn asr_failure(action: &str, error: &dyn std::fmt::Display) -> PortError {
    PortError::new(AppError::Asr).with_detail(format!("Parakeet could not {action}: {error}"))
}

/// Fails unless the session has every tensor name `io` lists.
fn check_names(session: &Session, io: &Io) -> Result<(), String> {
    let inputs: Vec<&str> = session.inputs().iter().map(|input| input.name()).collect();
    let outputs: Vec<&str> = session
        .outputs()
        .iter()
        .map(|output| output.name())
        .collect();
    let missing: Vec<&str> = io
        .inputs
        .iter()
        .filter(|name| !inputs.contains(name))
        .chain(io.outputs.iter().filter(|name| !outputs.contains(name)))
        .copied()
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "missing tensors {missing:?} (inputs {inputs:?}, outputs {outputs:?})"
        ))
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: joint signature check, recurrent state shape, int32 targets, logit count
 * WHAT:  Reads the recurrent state shape from `input_states_1` and checks both states agree, the token inputs are
 *        int32 and a static logit count equals the vocabulary plus the TDT durations.
 * WHY:   These are the facts the decode loop relies on; checking them once at load turns a mismatched export into a
 *        clear ModelCorrupt instead of a failed run on the user's first take.
 * WHERE: ParakeetModel::open.
 */
fn check_joint(joint: &Session, vocab_size: usize) -> Result<[usize; 3], String> {
    let input = |name: &str| joint.inputs().iter().find(|input| input.name() == name);
    let state = |name: &str| -> Result<[usize; 3], String> {
        let shape = input(name)
            .and_then(|input| input.dtype().tensor_shape())
            .ok_or_else(|| format!("{name} is not a tensor"))?;
        match shape[..] {
            [layers, _, hidden] if layers > 0 && hidden > 0 => Ok([
                usize::try_from(layers).map_err(|error| error.to_string())?,
                1,
                usize::try_from(hidden).map_err(|error| error.to_string())?,
            ]),
            _ => Err(format!(
                "{name} has shape {shape:?}, expected [layers, batch, hidden]"
            )),
        }
    };
    let state_shape = state("input_states_1")?;
    if state("input_states_2")? != state_shape {
        return Err("the two recurrent states differ in shape".to_owned());
    }
    for name in ["targets", "target_length"] {
        let element = input(name).and_then(|input| input.dtype().tensor_type());
        if element != Some(TensorElementType::Int32) {
            return Err(format!("{name} is {element:?}, expected Int32"));
        }
    }
    let logits = joint
        .outputs()
        .iter()
        .find(|output| output.name() == "outputs")
        .and_then(|output| output.dtype().tensor_shape())
        .and_then(|shape| shape.last().copied())
        .ok_or("`outputs` is not a tensor")?;
    let expected = vocab_size + DURATIONS.len();
    if logits > 0 && usize::try_from(logits).ok() != Some(expected) {
        return Err(format!(
            "`outputs` has {logits} logits, expected {expected}"
        ));
    }
    Ok(state_shape)
}
