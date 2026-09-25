/*!
 * SOURCE OF TRUTH KEYWORDS: replay journal, Replay, segment saved audio, retry segmentation, re-run VAD, deterministic retry, CaptureSummary from journal
 * WHAT:  `replay`: cuts a take's saved 16 kHz audio (from `journal::read`) into SpeechSegments with the same
 *        Segmenter a live take uses, and measures it like the capture worker does (CaptureSummary: duration,
 *        speech, segment count). Returns the segments, the summary and the detector for reuse.
 * WHY:   A retry must hear exactly what the first pass heard (05 A2): the journal holds the round-tripped 16-bit
 *        values the capture worker fed VAD and ASR, and cut points fall on whole VAD frames, so pushing the whole
 *        journal at once yields the same segments as the 10 ms pushes of the live take. Levels are not measured
 *        (nothing shows them for a retry), so the RMS fields stay 0; nothing was dropped, so `dropped_ms` is 0.
 * WHERE: pipeline/retry.rs (session_retry).
 */

use super::segmenter::Segmenter;
use crate::{
    ports::VoiceActivity,
    types::{CaptureSummary, PortResult, SegmentPolicy, SpeechSegment, samples_to_ms},
};

/// A journal cut into segments.
pub struct Replay {
    /// Segments to transcribe, in index order.
    pub segments: Vec<SpeechSegment>,
    /// What a live capture of the same audio would have measured (without levels).
    pub summary: CaptureSummary,
    /// The detector, for the next caller.
    pub vad: Box<dyn VoiceActivity>,
}

/// Segments `samples` (16 kHz mono, as `journal::read` returns them) with `vad` under `policy`.
pub fn replay(
    samples: &[f32],
    vad: Box<dyn VoiceActivity>,
    policy: SegmentPolicy,
) -> PortResult<Replay> {
    let mut segmenter = Segmenter::new(vad, policy)?;
    let mut segments = Vec::new();
    segmenter.push(samples, &mut |segment| segments.push(segment))?;
    let totals = segmenter.finish(&mut |segment| segments.push(segment));
    let summary = CaptureSummary {
        duration_ms: samples_to_ms(samples.len() as u64),
        speech_ms: totals.speech_ms,
        segments: totals.segments,
        ..CaptureSummary::default()
    };
    Ok(Replay {
        segments,
        summary,
        vad: totals.vad,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ports::fakes::FakeVoiceActivity, types::PIPELINE_SAMPLE_RATE_HZ};

    /// `ms` of 16 kHz samples at `level` (the fake detector calls anything loud speech).
    fn tone(ms: u64, level: f32) -> Vec<f32> {
        vec![level; usize::try_from(ms * u64::from(PIPELINE_SAMPLE_RATE_HZ) / 1000).unwrap()]
    }

    #[test]
    fn speech_between_pauses_becomes_ordered_segments_and_is_measured() {
        let mut audio = tone(1_000, 0.5);
        audio.extend(tone(1_000, 0.0));
        audio.extend(tone(800, 0.5));
        audio.extend(tone(200, 0.0));

        let replayed = replay(
            &audio,
            Box::new(FakeVoiceActivity::new(32)),
            SegmentPolicy::DEFAULT,
        )
        .unwrap();
        let indexes: Vec<u32> = replayed
            .segments
            .iter()
            .map(|segment| segment.index)
            .collect();
        assert_eq!(indexes, [0, 1]);
        assert_eq!(replayed.summary.duration_ms, 3_000);
        assert_eq!(replayed.summary.segments, 2);
        assert!(
            (1_700..=1_900).contains(&replayed.summary.speech_ms),
            "{}",
            replayed.summary.speech_ms
        );
        assert_eq!(replayed.summary.dropped_ms, 0);
    }

    #[test]
    fn replay_is_deterministic_however_the_audio_arrived() {
        let mut audio = tone(700, 0.5);
        audio.extend(tone(900, 0.0));
        audio.extend(tone(1_300, 0.5));
        let first = replay(
            &audio,
            Box::new(FakeVoiceActivity::new(32)),
            SegmentPolicy::DEFAULT,
        )
        .unwrap();
        let second = replay(&audio, first.vad, SegmentPolicy::DEFAULT).unwrap();
        assert_eq!(first.segments, second.segments);
        assert_eq!(first.summary, second.summary);
    }

    #[test]
    fn silence_gives_no_segments() {
        let replayed = replay(
            &tone(2_000, 0.0),
            Box::new(FakeVoiceActivity::new(32)),
            SegmentPolicy::DEFAULT,
        )
        .unwrap();
        assert!(replayed.segments.is_empty());
        assert_eq!(replayed.summary.speech_ms, 0);
        assert_eq!(replayed.summary.duration_ms, 2_000);
    }
}
