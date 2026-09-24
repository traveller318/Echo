/*!
 * SOURCE OF TRUTH KEYWORDS: TDT greedy decode, token-and-duration transducer, Joint, joint network step, duration logits, blank, max symbols per frame
 * WHAT:  `greedy_decode` runs the Token-and-Duration Transducer greedy search over `frames` encoder frames through a
 *        Joint (the prediction + joint network, one call per step) and returns the emitted token ids.
 * WHY:   Parakeet TDT's joint output is the token logits followed by duration logits; the duration says how many
 *        encoder frames the prediction covers, so decoding skips frames instead of visiting each one (faster than
 *        plain RNN-T greedy and what the model was trained for). The loop matches the reference implementation
 *        (onnx-asr): the prediction state advances only when a real token is emitted; a frame moves on when the
 *        duration is non-zero, when the token is blank, or after `max_symbols` tokens on one frame, so the loop
 *        always terminates. The Joint trait keeps this pure: tests drive it with scripted logits, the adapter with
 *        ONNX Runtime.
 * WHERE: Called by ParakeetOnnx::transcribe (parakeet_onnx/model.rs) with its OnnxJoint.
 */

use crate::types::{AppError, PortError, PortResult};

/// Encoder frames each duration class skips (Parakeet TDT v2/v3 are trained with 0–4).
pub const DURATIONS: [usize; 5] = [0, 1, 2, 3, 4];

/// Tokens emitted on one frame before the decoder is forced forward (reference value).
pub const MAX_SYMBOLS_PER_FRAME: usize = 10;

/// The prediction + joint network, one step at a time.
pub trait Joint {
    /// Logits (tokens, then durations) for encoder `frame` after `last_token`, from the committed prediction state.
    /// The state the call produced is kept as a candidate until `commit`.
    fn logits(&mut self, frame: usize, last_token: usize) -> PortResult<&[f32]>;

    /// Makes the candidate state of the last `logits` call current: a real token was emitted.
    fn commit(&mut self);
}

/// Greedy TDT search over `frames` frames; `vocab_size` counts the blank at id `blank`.
pub fn greedy_decode(
    joint: &mut dyn Joint,
    frames: usize,
    vocab_size: usize,
    blank: usize,
) -> PortResult<Vec<usize>> {
    let mut tokens = Vec::new();
    let mut frame = 0;
    let mut emitted = 0;
    while frame < frames {
        let last = tokens.last().copied().unwrap_or(blank);
        let logits = joint.logits(frame, last)?;
        if logits.len() != vocab_size + DURATIONS.len() {
            return Err(PortError::new(AppError::Asr).with_detail(format!(
                "the joint network returned {} logits, expected {vocab_size} tokens + {} durations",
                logits.len(),
                DURATIONS.len()
            )));
        }
        let (token_logits, duration_logits) = logits.split_at(vocab_size);
        let token = argmax(token_logits);
        let skip = DURATIONS[argmax(duration_logits)];
        if token != blank {
            joint.commit();
            tokens.push(token);
            emitted += 1;
        }
        if skip > 0 {
            frame += skip;
            emitted = 0;
        } else if token == blank || emitted == MAX_SYMBOLS_PER_FRAME {
            frame += 1;
            emitted = 0;
        }
    }
    Ok(tokens)
}

/// Index of the largest value (the first on ties; NaN never wins; 0 when every value is NaN).
fn argmax(values: &[f32]) -> usize {
    let mut best: Option<(usize, f32)> = None;
    for (index, value) in values.iter().copied().enumerate() {
        if !value.is_nan() && best.is_none_or(|(_, top)| value > top) {
            best = Some((index, value));
        }
    }
    best.map_or(0, |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    const VOCAB: usize = 4;
    const BLANK: usize = 3;

    /// Answers each step with the next scripted (token, duration class) and records the calls.
    struct Scripted {
        steps: VecDeque<(usize, usize)>,
        logits: Vec<f32>,
        calls: Vec<(usize, usize)>,
        commits: usize,
    }

    impl Scripted {
        fn new(steps: &[(usize, usize)]) -> Self {
            Self {
                steps: steps.iter().copied().collect(),
                logits: Vec::new(),
                calls: Vec::new(),
                commits: 0,
            }
        }
    }

    impl Joint for Scripted {
        fn logits(&mut self, frame: usize, last_token: usize) -> PortResult<&[f32]> {
            self.calls.push((frame, last_token));
            let (token, duration) = self.steps.pop_front().unwrap_or((BLANK, 1));
            self.logits = vec![0.0; VOCAB + DURATIONS.len()];
            self.logits[token] = 1.0;
            self.logits[VOCAB + duration] = 1.0;
            Ok(&self.logits)
        }

        fn commit(&mut self) {
            self.commits += 1;
        }
    }

    #[test]
    fn durations_skip_frames_and_tokens_feed_the_next_step() {
        // Frame 0: token 1, skip 2 → frame 2: token 2, stay → frame 2: blank, skip 1 → frame 3: blank, skip 1.
        let mut joint = Scripted::new(&[(1, 2), (2, 0), (BLANK, 1), (BLANK, 1)]);
        let tokens = greedy_decode(&mut joint, 4, VOCAB, BLANK).unwrap();
        assert_eq!(tokens, [1, 2]);
        assert_eq!(joint.calls, [(0, BLANK), (2, 1), (2, 2), (3, 2)]);
        assert_eq!(joint.commits, 2, "state advances only on real tokens");
    }

    #[test]
    fn a_blank_with_zero_duration_still_moves_forward() {
        let mut joint = Scripted::new(&[(BLANK, 0), (BLANK, 0)]);
        assert!(
            greedy_decode(&mut joint, 2, VOCAB, BLANK)
                .unwrap()
                .is_empty()
        );
        assert_eq!(joint.calls, [(0, BLANK), (1, BLANK)]);
    }

    #[test]
    fn a_frame_emits_at_most_max_symbols() {
        let mut joint = Scripted::new(&[(1, 0); 20]);
        let tokens = greedy_decode(&mut joint, 1, VOCAB, BLANK).unwrap();
        assert_eq!(tokens.len(), MAX_SYMBOLS_PER_FRAME);
    }

    #[test]
    fn a_wrong_logit_count_is_an_asr_error() {
        struct Short;
        impl Joint for Short {
            fn logits(&mut self, _: usize, _: usize) -> PortResult<&[f32]> {
                Ok(&[0.0; 3])
            }
            fn commit(&mut self) {}
        }
        assert_eq!(
            greedy_decode(&mut Short, 1, VOCAB, BLANK)
                .err()
                .map(PortError::into_app_error),
            Some(AppError::Asr)
        );
        assert!(
            greedy_decode(&mut Short, 0, VOCAB, BLANK)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn argmax_prefers_the_first_maximum_and_ignores_nan() {
        assert_eq!(argmax(&[0.1, 0.5, 0.5]), 1);
        assert_eq!(argmax(&[f32::NAN, 0.2, 0.1]), 1);
        assert_eq!(argmax(&[f32::NAN, f32::NAN]), 0);
        assert_eq!(argmax(&[0.3, f32::NAN, 0.4]), 2);
    }
}
