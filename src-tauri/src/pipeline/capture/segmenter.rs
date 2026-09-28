/*!
 * SOURCE OF TRUTH KEYWORDS: Segmenter, VAD framing, exact VAD frames, frame remainder, segment on pause, soft pause cut, max segment cut, pre-roll, speech ms
 * WHAT:  Segmenter feeds the 16 kHz stream to a VoiceActivity detector in exact frames (the remainder waits for
 *        the next push) and cuts the take into SpeechSegments by SegmentPolicy: on a pause of at least
 *        `min_pause_ms` after speech (`soft_pause_ms` once the segment holds `soft_after_ms`), or at
 *        `max_segment_ms` (in the latest pause of the segment's second half, else hard). It also totals the take's
 *        speech time.
 * WHY:   Silero takes exactly 512-sample frames and is stateful, so frames are never padded or split and the
 *        detector is reset once per take, here in `new`, never mid-take (05 A11). Segments end in silence so
 *        words are never cut and each can be transcribed while the user keeps speaking (02 §6.1); the soft pause
 *        keeps the segment still open at the stop short, since it alone is transcribed after the stop. Silence before
 *        speech is trimmed to `pre_roll_ms`, so a long quiet start costs no ASR time; a segment with less than
 *        `min_speech_ms` of speech is not emitted (clicks make ASR invent words, 05 A4), but its speech still
 *        counts toward the take's total. Cut points fall on frame boundaries, so each emitted segment is exactly
 *        the journal's audio for that stretch; only the final one may include the sub-frame remainder.
 * WHERE: Owned by the capture worker (pipeline/capture/mod.rs); `finish` hands the detector back so the session
 *        reuses it for the next take.
 */

use crate::{
    ports::VoiceActivity,
    types::{AppError, PortError, PortResult, SegmentPolicy, SpeechSegment, VadEvent},
};

/// What a finished segmenter reports.
pub struct SegmenterTotals {
    /// The detector, ready to be reset for the next take.
    pub vad: Box<dyn VoiceActivity>,
    /// Speech across the whole take, in ms.
    pub speech_ms: u64,
    /// Segments emitted.
    pub segments: u32,
}

/// Frames the policy's millisecond values come to.
#[derive(Debug, Clone, Copy)]
struct FramePolicy {
    pause: usize,
    soft_after: usize,
    soft_pause: usize,
    max: usize,
    pre_roll: usize,
    min_speech: usize,
}

impl FramePolicy {
    fn new(policy: SegmentPolicy, frame_ms: u32) -> Self {
        let ceil = |ms: u32| ms.div_ceil(frame_ms) as usize;
        let pause = ceil(policy.min_pause_ms).max(1);
        Self {
            pause,
            soft_after: ceil(policy.soft_after_ms),
            // Never longer than the normal pause, so a long segment is never harder to close than a short one.
            soft_pause: ceil(policy.soft_pause_ms).clamp(1, pause),
            // At least two frames, so a forced cut always leaves room for a pause in the second half.
            max: ((policy.max_segment_ms / frame_ms) as usize).max(2),
            pre_roll: ceil(policy.pre_roll_ms),
            min_speech: ceil(policy.min_speech_ms).max(1),
        }
    }
}

/// Cuts a take into speech segments while it is recorded.
pub struct Segmenter {
    vad: Box<dyn VoiceActivity>,
    frame: usize,
    frame_ms: u32,
    policy: FramePolicy,
    /// Samples not yet classified (fewer than one frame after each push).
    pending: Vec<f32>,
    /// The open segment: whole frames only, with one verdict per frame.
    samples: Vec<f32>,
    verdicts: Vec<VadEvent>,
    segment_speech: usize,
    silence_run: usize,
    /// Position of `samples[0]` in the take, in frames.
    start_frame: u64,
    next_index: u32,
    speech_frames: u64,
}

impl Segmenter {
    /// Resets `vad` for a new take; fails if it declares a frame of no samples.
    pub fn new(mut vad: Box<dyn VoiceActivity>, policy: SegmentPolicy) -> PortResult<Self> {
        vad.reset()?;
        let caps = vad.caps();
        let frame = caps.frame_samples();
        if frame == 0 || caps.frame_ms == 0 {
            return Err(PortError::new(AppError::Internal)
                .with_detail(format!("the VAD declares an empty frame ({caps:?})")));
        }
        Ok(Self {
            vad,
            frame,
            frame_ms: caps.frame_ms,
            policy: FramePolicy::new(policy, caps.frame_ms),
            pending: Vec::with_capacity(frame * 2),
            samples: Vec::new(),
            verdicts: Vec::new(),
            segment_speech: 0,
            silence_run: 0,
            start_frame: 0,
            next_index: 0,
            speech_frames: 0,
        })
    }

    /// Classifies every whole frame in `pending + samples` and emits the segments that close.
    pub fn push(&mut self, samples: &[f32], emit: &mut dyn FnMut(SpeechSegment)) -> PortResult<()> {
        self.pending.extend_from_slice(samples);
        let mut offset = 0;
        while self.pending.len() - offset >= self.frame {
            let frame = &self.pending[offset..offset + self.frame];
            let verdict = self.vad.push(frame)?;
            self.samples.extend_from_slice(frame);
            offset += self.frame;
            self.on_frame(verdict, emit);
        }
        self.pending.drain(..offset);
        Ok(())
    }

    /// Emits the open segment (with the unclassified remainder) if it holds enough speech.
    pub fn finish(mut self, emit: &mut dyn FnMut(SpeechSegment)) -> SegmenterTotals {
        if self.segment_speech >= self.policy.min_speech {
            let mut samples = std::mem::take(&mut self.samples);
            samples.extend_from_slice(&self.pending);
            let speech = self.segment_speech;
            self.emit_segment(samples, speech, emit);
        }
        SegmenterTotals {
            vad: self.vad,
            speech_ms: self.speech_frames * u64::from(self.frame_ms),
            segments: self.next_index,
        }
    }

    fn on_frame(&mut self, verdict: VadEvent, emit: &mut dyn FnMut(SpeechSegment)) {
        self.verdicts.push(verdict);
        match verdict {
            VadEvent::Speech => {
                self.segment_speech += 1;
                self.speech_frames += 1;
                self.silence_run = 0;
            }
            VadEvent::Silence => self.silence_run += 1,
        }
        if self.segment_speech == 0 {
            self.trim_to_pre_roll();
        } else if self.silence_run >= self.pause_to_close() {
            self.cut(self.verdicts.len(), emit);
        } else if self.verdicts.len() >= self.policy.max {
            let at = self.forced_cut_point();
            self.cut(at, emit);
        }
    }

    /// Silent frames that close the open segment: the soft pause once it holds `soft_after` frames.
    fn pause_to_close(&self) -> usize {
        if self.verdicts.len() >= self.policy.soft_after {
            self.policy.soft_pause
        } else {
            self.policy.pause
        }
    }

    /// Where a segment at its length limit is cut: after the latest silent frame in its second half, if any.
    fn forced_cut_point(&self) -> usize {
        let half = self.verdicts.len() / 2;
        self.verdicts[half..]
            .iter()
            .rposition(|verdict| *verdict == VadEvent::Silence)
            .map_or(self.verdicts.len(), |position| half + position + 1)
    }

    /// Closes the first `frames` frames as a segment; the rest opens the next one.
    fn cut(&mut self, frames: usize, emit: &mut dyn FnMut(SpeechSegment)) {
        let head: Vec<f32> = self.samples.drain(..frames * self.frame).collect();
        let speech = self.verdicts[..frames]
            .iter()
            .filter(|verdict| **verdict == VadEvent::Speech)
            .count();
        self.verdicts.drain(..frames);
        self.segment_speech -= speech;
        self.silence_run = self
            .verdicts
            .iter()
            .rev()
            .take_while(|verdict| **verdict == VadEvent::Silence)
            .count();
        if speech >= self.policy.min_speech {
            self.emit_segment(head, speech, emit);
        }
        self.start_frame += frames as u64;
        if self.segment_speech == 0 {
            self.trim_to_pre_roll();
        }
    }

    /// Drops leading silence beyond the pre-roll from a segment that holds no speech yet.
    fn trim_to_pre_roll(&mut self) {
        let excess = self.verdicts.len().saturating_sub(self.policy.pre_roll);
        if excess > 0 {
            self.samples.drain(..excess * self.frame);
            self.verdicts.drain(..excess);
            self.start_frame += excess as u64;
        }
    }

    fn emit_segment(
        &mut self,
        samples: Vec<f32>,
        speech_frames: usize,
        emit: &mut dyn FnMut(SpeechSegment),
    ) {
        let frame_ms = u64::from(self.frame_ms);
        emit(SpeechSegment {
            index: self.next_index,
            start_ms: self.start_frame * frame_ms,
            speech_ms: u32::try_from(speech_frames as u64 * frame_ms).unwrap_or(u32::MAX),
            samples,
        });
        self.next_index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::fakes::FakeVoiceActivity;

    const FRAME_MS: u32 = 32;
    const FRAME: usize = 512;

    fn policy() -> SegmentPolicy {
        SegmentPolicy {
            min_pause_ms: 320,
            soft_after_ms: 1_600,
            soft_pause_ms: 128,
            max_segment_ms: 3_200,
            pre_roll_ms: 64,
            min_speech_ms: 64,
        }
    }

    fn speech(frames: usize) -> Vec<f32> {
        vec![0.5; frames * FRAME]
    }

    fn silence(frames: usize) -> Vec<f32> {
        vec![0.0; frames * FRAME]
    }

    struct Run {
        probe: FakeVoiceActivity,
        segmenter: Segmenter,
        segments: Vec<SpeechSegment>,
    }

    impl Run {
        fn new(policy: SegmentPolicy) -> Self {
            let probe = FakeVoiceActivity::new(FRAME_MS);
            let segmenter = Segmenter::new(Box::new(probe.clone()), policy).unwrap();
            Self {
                probe,
                segmenter,
                segments: Vec::new(),
            }
        }

        /// Pushes `audio` in uneven slices that never line up with frames.
        fn push(&mut self, audio: &[f32]) {
            for slice in audio.chunks(300) {
                let segments = &mut self.segments;
                self.segmenter
                    .push(slice, &mut |segment| segments.push(segment))
                    .unwrap();
            }
        }

        fn finish(self) -> (Vec<SpeechSegment>, SegmenterTotals, FakeVoiceActivity) {
            let mut segments = self.segments;
            let totals = self.segmenter.finish(&mut |segment| segments.push(segment));
            (segments, totals, self.probe)
        }
    }

    #[test]
    fn frames_are_exact_and_the_remainder_waits_for_more_audio() {
        let mut run = Run::new(policy());
        assert_eq!(
            run.probe.resets(),
            1,
            "reset once, at the start of the take"
        );
        run.push(&vec![0.0; FRAME - 1]);
        assert_eq!(run.probe.frames(), 0);
        run.push(&[0.0]);
        assert_eq!(run.probe.frames(), 1);
        run.push(&vec![0.0; FRAME * 3 + 17]);
        assert_eq!(run.probe.frames(), 4);
        assert_eq!(run.segmenter.pending.len(), 17);
        let (_, _, probe) = run.finish();
        assert_eq!(probe.resets(), 1, "never reset mid-take");
    }

    #[test]
    fn a_pause_closes_a_segment_that_keeps_its_pre_roll_and_pause() {
        let mut run = Run::new(policy());
        run.push(&silence(20));
        run.push(&speech(10));
        run.push(&silence(10));
        assert_eq!(run.segments.len(), 1, "10 silent frames = 320 ms closes it");
        let first = &run.segments[0];
        assert_eq!(first.index, 0);
        // 2 frames of pre-roll + 10 speech + 10 pause.
        assert_eq!(first.samples.len(), 22 * FRAME);
        assert_eq!(first.start_ms, 18 * u64::from(FRAME_MS));
        assert_eq!(first.speech_ms, 10 * FRAME_MS);

        run.push(&speech(5));
        let (segments, totals, _) = run.finish();
        assert_eq!(segments.len(), 2, "the open segment is flushed on finish");
        assert_eq!(segments[1].index, 1);
        assert_eq!(totals.segments, 2);
        assert_eq!(totals.speech_ms, 15 * u64::from(FRAME_MS));
    }

    #[test]
    fn long_speech_is_cut_in_its_latest_pause_or_hard_at_the_limit() {
        let mut run = Run::new(policy());
        // 100 frames max: speech with a short (non-closing) pause at frames 70..73.
        run.push(&speech(70));
        run.push(&silence(3));
        run.push(&speech(40));
        assert_eq!(run.segments.len(), 1);
        assert_eq!(
            run.segments[0].samples.len(),
            73 * FRAME,
            "cut after the pause"
        );

        let mut hard = Run::new(policy());
        hard.push(&speech(250));
        assert_eq!(hard.segments.len(), 2);
        assert!(
            hard.segments
                .iter()
                .all(|segment| segment.samples.len() == 100 * FRAME)
        );
        assert_eq!(hard.segments[1].start_ms, 100 * u64::from(FRAME_MS));
    }

    #[test]
    fn a_long_segment_closes_on_the_soft_pause() {
        let mut run = Run::new(policy());
        // Past 50 frames (soft_after), 4 silent frames (128 ms) close the segment instead of 10.
        run.push(&speech(60));
        run.push(&silence(3));
        assert!(run.segments.is_empty(), "3 silent frames are not a pause");
        run.push(&silence(1));
        assert_eq!(run.segments.len(), 1);
        assert_eq!(run.segments[0].samples.len(), 64 * FRAME);
        assert_eq!(run.segments[0].speech_ms, 60 * FRAME_MS);

        // The next segment is short again, so it waits for the full pause.
        run.push(&speech(20));
        run.push(&silence(4));
        assert_eq!(
            run.segments.len(),
            1,
            "a short segment ignores the soft pause"
        );
        run.push(&silence(6));
        assert_eq!(run.segments.len(), 2);
        assert_eq!(run.segments[1].start_ms, 64 * u64::from(FRAME_MS));
        assert_eq!(run.segments[1].samples.len(), 30 * FRAME);
    }

    #[test]
    fn a_soft_pause_above_the_normal_pause_is_capped_to_it() {
        let mut run = Run::new(SegmentPolicy {
            soft_pause_ms: 640,
            ..policy()
        });
        run.push(&speech(60));
        run.push(&silence(10));
        assert_eq!(
            run.segments.len(),
            1,
            "the normal 320 ms pause still closes it"
        );
    }

    #[test]
    fn clicks_are_not_transcribed_but_still_count_as_speech() {
        let mut run = Run::new(policy());
        run.push(&speech(1));
        run.push(&silence(12));
        let (segments, totals, _) = run.finish();
        assert!(segments.is_empty());
        assert_eq!(totals.speech_ms, u64::from(FRAME_MS));
        assert_eq!(totals.segments, 0);
    }

    #[test]
    fn a_silent_take_emits_nothing_and_keeps_only_the_pre_roll() {
        let mut run = Run::new(policy());
        run.push(&silence(500));
        assert!(run.segmenter.samples.len() <= 2 * FRAME);
        let (segments, totals, _) = run.finish();
        assert!(segments.is_empty());
        assert_eq!(totals.speech_ms, 0);
    }

    #[test]
    fn the_final_segment_keeps_the_sub_frame_remainder() {
        let mut run = Run::new(policy());
        run.push(&speech(4));
        run.push(&[0.5; 100]);
        let (segments, _, _) = run.finish();
        assert_eq!(segments[0].samples.len(), 4 * FRAME + 100);
    }

    #[test]
    fn a_detector_failure_is_returned() {
        struct Broken;
        impl VoiceActivity for Broken {
            fn caps(&self) -> crate::types::VadCaps {
                crate::types::VadCaps { frame_ms: FRAME_MS }
            }
            fn reset(&mut self) -> PortResult<()> {
                Ok(())
            }
            fn push(&mut self, _: &[f32]) -> PortResult<VadEvent> {
                Err(PortError::new(AppError::Internal).with_detail("inference failed"))
            }
        }
        let mut segmenter = Segmenter::new(Box::new(Broken), policy()).unwrap();
        assert!(segmenter.push(&speech(1), &mut |_| {}).is_err());
    }
}
