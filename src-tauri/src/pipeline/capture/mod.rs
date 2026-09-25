/*!
 * SOURCE OF TRUTH KEYWORDS: capture worker, Capture, CaptureConfig, CaptureOutcome, RingSink, rtrb ring, drain loop, AudioLevel emit, journal write, speech segments
 * WHAT:  One take's audio path (02 §6.1): `Capture::start` opens the microphone through the AudioCapture port with a
 *        lock-free ring as its sink and starts the capture worker thread, which drains the ring, converts to 16 kHz
 *        mono, writes the WAV journal, sends AudioLevel (≤ 30 Hz), and cuts speech segments with the VAD.
 *        `pause`/`resume` hold the microphone for the Esc countdown; `finish` closes it, lets the worker drain and
 *        flush everything, and returns the CaptureOutcome (summary, the detector for the next take, any failure).
 * WHY:   The device callback may not allocate, lock or block (02 §6.1), so the sink only copies whole callback
 *        buffers into an rtrb ring (a buffer that does not fit is dropped whole and counted, which keeps channels
 *        aligned); everything else runs on the worker, above normal priority (05 A9). The ring is sized from the
 *        adapter's caps before the stream exists, because the sink must be handed to `start`. Each stage is optional,
 *        so the same worker serves a take (journal + levels + segments) and the microphone check (levels only).
 *        A stage failure (disk full, VAD error) is reported once through the stream's CaptureEvent sink, the worker
 *        keeps draining so the device never stalls, and the journal is always finalized, so what was captured is
 *        kept (00 constraint 5). Dropping a Capture without `finish` (an error path, a panic) still closes the mic
 *        and finalizes the journal. The worker wakes every 10 ms only while a take is open: no timers when idle.
 * WHERE: Started by the session actor on RecordPressed (after the transcripts row exists, 02 §7.3) and by
 *        `check_microphone` (mic_check.rs) for `audio_test_level`; segments go to the ASR worker's sink. `replay`
 *        (replay.rs) runs the same segmenter over a saved journal for session_retry.
 */

mod convert;
pub mod journal;
mod level;
mod mic_check;
mod replay;
mod segmenter;

pub use mic_check::check_microphone;
pub use replay::{Replay, replay};

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use rtrb::{Consumer, Producer, RingBuffer};

use self::{
    convert::{Converter, from_pcm16, to_pcm16},
    journal::Journal,
    level::LevelMeter,
    segmenter::Segmenter,
};
use crate::{
    ports::{AudioCapture, AudioSink, CaptureStream, EventSink, VoiceActivity, WorkerScheduler},
    types::{
        AppError, AppEvent, AudioCaps, AudioDeviceId, AudioLevel, CaptureEvent, CaptureFormat,
        CaptureSummary, PortError, PortResult, SegmentPolicy, SpeechSegment, WorkerPriority,
        samples_to_ms,
    },
};

/// How often the worker drains the ring while a take is open.
const DRAIN_INTERVAL: Duration = Duration::from_millis(10);

/// Device audio the ring holds, at the largest format the adapter declares.
const RING_SECONDS: usize = 2;

/// Bounds on the ring, in samples (128 KiB to 4 MiB of f32): 2 s of any 48 kHz stereo mic, and still about 0.7 s
/// for a 192 kHz 8-channel array, far more than the worker's 10 ms drain interval needs.
const RING_MIN_SAMPLES: usize = 1 << 15;
const RING_MAX_SAMPLES: usize = 1 << 20;

/// Device frames the worker reads per pass.
const READ_BLOCK_FRAMES: usize = 2048;

/// Voice activity detection for a take: the detector, how to cut, and where segments go.
pub struct Segmentation {
    pub vad: Box<dyn VoiceActivity>,
    pub policy: SegmentPolicy,
    pub segments: Arc<dyn EventSink<SpeechSegment>>,
}

/// What a capture does with the audio, and where it reports.
pub struct CaptureConfig {
    /// Where the WAV journal is written (`AppPaths::recording`); None writes none.
    pub journal: Option<PathBuf>,
    /// Segment the take for ASR; None skips voice activity detection.
    pub segmentation: Option<Segmentation>,
    /// Receives AudioLevel while samples flow; None sends none.
    pub levels: Option<Arc<dyn EventSink<AppEvent>>>,
    /// Receives device loss and stream failures from the adapter, and worker failures.
    pub events: Arc<dyn EventSink<CaptureEvent>>,
    /// Raises the worker thread's priority (05 A9).
    pub scheduler: Arc<dyn WorkerScheduler>,
}

/// What a finished capture hands back.
pub struct CaptureOutcome {
    /// Measurements of everything captured, also when a stage failed.
    pub summary: CaptureSummary,
    /// The detector, for the next take; None when there was none or the worker failed (rebuild it from the registry).
    pub vad: Option<Box<dyn VoiceActivity>>,
    /// The first stage failure, already reported through `CaptureConfig::events`.
    pub failure: Option<PortError>,
}

impl CaptureOutcome {
    fn lost(detail: &str) -> Self {
        Self {
            summary: CaptureSummary::default(),
            vad: None,
            failure: Some(PortError::new(AppError::Internal).with_detail(detail.to_owned())),
        }
    }
}

/// Sent to the worker when the stream has stopped and it may finish.
struct Finish;

/// An open take: the device stream and the worker processing it.
pub struct Capture {
    stream: Option<Box<dyn CaptureStream>>,
    format: CaptureFormat,
    finish: mpsc::Sender<Finish>,
    worker: Option<JoinHandle<CaptureOutcome>>,
}

impl Capture {
    /**
     * SOURCE OF TRUTH KEYWORDS: Capture::start, open take, create journal before mic, spawn capture worker
     * WHAT:  Creates the journal and resets the detector, opens `device` (None = Windows default) with a ring sink,
     *        and starts the worker; returns the open take.
     * WHY:   Stages that can fail on their own (the journal file, the detector reset) are prepared before the
     *        microphone opens, so a failure never leaves a live stream behind. Errors keep the port's meaning
     *        (`NotFound { audio_device }`, `PermissionDenied { microphone }`, `AudioDevice`, `Storage`).
     * WHERE: Session actor (RecordPressed); check_microphone.
     */
    pub fn start(
        capture: &dyn AudioCapture,
        device: Option<&AudioDeviceId>,
        config: CaptureConfig,
    ) -> PortResult<Self> {
        let CaptureConfig {
            journal,
            segmentation,
            levels,
            events,
            scheduler,
        } = config;
        let journal = journal.as_deref().map(Journal::create).transpose()?;
        let segmenter = segmentation
            .map(|segmentation| {
                Segmenter::new(segmentation.vad, segmentation.policy)
                    .map(|segmenter| (segmenter, segmentation.segments))
            })
            .transpose()?;

        let (producer, consumer) = RingBuffer::new(ring_capacity(&capture.caps()));
        let dropped = Arc::new(AtomicU64::new(0));
        let sink = RingSink {
            producer,
            dropped: Arc::clone(&dropped),
        };
        let stream = capture.start(device, Box::new(sink), Arc::clone(&events))?;
        let format = stream.format();
        let converter = Converter::new(format)?;

        let worker = Worker {
            consumer,
            format,
            dropped,
            converter,
            journal,
            meter: LevelMeter::default(),
            segmenter,
            levels,
            events,
            samples_out: 0,
            failure: None,
            converted: Vec::new(),
            pcm: Vec::new(),
        };
        let (finish, finish_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("echo-capture".to_owned())
            .spawn(move || {
                if let Err(error) = scheduler.prioritize_current_thread(WorkerPriority::AboveNormal)
                {
                    tracing::warn!(
                        detail = error.detail(),
                        "capture worker runs at default priority"
                    );
                }
                worker.run(&finish_rx)
            })
            .map_err(|error| {
                PortError::new(AppError::Internal)
                    .with_detail(format!("capture worker could not start: {error}"))
            })?;
        Ok(Self {
            stream: Some(stream),
            format,
            finish,
            worker: Some(handle),
        })
    }

    /// The device format of the open stream.
    pub fn format(&self) -> CaptureFormat {
        self.format
    }

    /// Stops delivering audio but keeps the device open (the Esc countdown).
    pub fn pause(&mut self) -> PortResult<()> {
        self.stream_mut()?.pause()
    }

    /// Delivers audio again after `pause`.
    pub fn resume(&mut self) -> PortResult<()> {
        self.stream_mut()?.resume()
    }

    /// Closes the microphone, waits for the worker to process and flush everything, and returns the outcome.
    pub fn finish(mut self) -> CaptureOutcome {
        self.shutdown()
    }

    fn stream_mut(&mut self) -> PortResult<&mut Box<dyn CaptureStream>> {
        self.stream.as_mut().ok_or_else(|| {
            PortError::new(AppError::AudioDevice)
                .with_detail("the capture stream is already closed")
        })
    }

    /// Stops the stream (every captured sample reaches the ring first), then lets the worker drain and join it.
    fn shutdown(&mut self) -> CaptureOutcome {
        if let Some(stream) = self.stream.take()
            && let Err(error) = stream.stop()
        {
            // A lost device cannot stop cleanly; what reached the ring is still processed.
            tracing::warn!(
                detail = error.detail(),
                "capture stream did not stop cleanly"
            );
        }
        // The worker also finishes when this sender is gone, so a failed send needs no handling.
        let _ = self.finish.send(Finish);
        match self.worker.take() {
            Some(handle) => handle
                .join()
                .unwrap_or_else(|_| CaptureOutcome::lost("capture worker panicked")),
            None => CaptureOutcome::lost("capture already finished"),
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let outcome = self.shutdown();
            if let Some(failure) = outcome.failure {
                tracing::warn!(
                    detail = failure.detail(),
                    "capture closed without finish after a failure"
                );
            }
        }
    }
}

/// Ring size for the largest format the adapter can open.
fn ring_capacity(caps: &AudioCaps) -> usize {
    let rate = caps.sample_rates.iter().copied().max().unwrap_or(48_000) as usize;
    let channels = usize::from(caps.channels.iter().copied().max().unwrap_or(2));
    (rate * channels * RING_SECONDS).clamp(RING_MIN_SAMPLES, RING_MAX_SAMPLES)
}

/**
 * SOURCE OF TRUTH KEYWORDS: RingSink, real-time sink, no allocation callback, whole buffer push, dropped samples
 * WHAT:  The AudioSink the device callback pushes into: copies a whole callback buffer into the ring, or drops it
 *        whole and counts the samples when it does not fit.
 * WHY:   Runs on the real-time thread: `push_entire_slice` copies without allocating or locking. Dropping whole
 *        buffers keeps every ring read aligned to device frames, so a stall costs audio, never channel order.
 * WHERE: Created by Capture::start; moved onto the adapter's callback thread.
 */
struct RingSink {
    producer: Producer<f32>,
    dropped: Arc<AtomicU64>,
}

impl AudioSink for RingSink {
    fn push(&mut self, samples: &[f32]) {
        if self.producer.push_entire_slice(samples).is_err() {
            self.dropped
                .fetch_add(samples.len() as u64, Ordering::Relaxed);
        }
    }
}

/// The capture worker's state; lives on its own thread for one take.
struct Worker {
    consumer: Consumer<f32>,
    format: CaptureFormat,
    dropped: Arc<AtomicU64>,
    converter: Converter,
    journal: Option<Journal>,
    meter: LevelMeter,
    segmenter: Option<(Segmenter, Arc<dyn EventSink<SpeechSegment>>)>,
    levels: Option<Arc<dyn EventSink<AppEvent>>>,
    events: Arc<dyn EventSink<CaptureEvent>>,
    samples_out: u64,
    failure: Option<PortError>,
    converted: Vec<f32>,
    pcm: Vec<i16>,
}

impl Worker {
    fn run(mut self, finish: &mpsc::Receiver<Finish>) -> CaptureOutcome {
        let channels = usize::from(self.format.channels).max(1);
        let mut raw = vec![0.0; READ_BLOCK_FRAMES * channels];
        loop {
            let finishing = !matches!(
                finish.recv_timeout(DRAIN_INTERVAL),
                Err(RecvTimeoutError::Timeout)
            );
            self.drain(&mut raw, channels);
            if finishing {
                return self.finish();
            }
        }
    }

    /// Processes everything in the ring, in whole device frames.
    fn drain(&mut self, raw: &mut [f32], channels: usize) {
        loop {
            let available = self.consumer.slots().min(raw.len());
            let whole = available - available % channels;
            if whole == 0 {
                return;
            }
            let read = self.consumer.pop_partial_slice(&mut raw[..whole]).0.len();
            self.process(&raw[..read]);
        }
    }

    fn process(&mut self, device_samples: &[f32]) {
        if self.failure.is_some() {
            // Keep draining so the device never backs up; the take is already reported as failed.
            return;
        }
        self.converted.clear();
        match self.converter.push(device_samples, &mut self.converted) {
            Ok(()) => self.consume_converted(),
            Err(error) => self.fail(error),
        }
    }

    /// Runs the converted 16 kHz block through the journal, the level meter and the segmenter.
    fn consume_converted(&mut self) {
        // Round-trip through the journal format, so VAD and ASR hear exactly what the journal keeps (05 A2).
        self.pcm.clear();
        for sample in &mut self.converted {
            let stored = to_pcm16(*sample);
            self.pcm.push(stored);
            *sample = from_pcm16(stored);
        }
        self.samples_out += self.converted.len() as u64;
        if let Some(journal) = &mut self.journal
            && let Err(error) = journal.append(&self.pcm)
        {
            return self.fail(error);
        }
        let levels = &self.levels;
        self.meter.push(&self.converted, |rms| {
            if let Some(levels) = levels {
                levels.emit(AudioLevel { rms }.into());
            }
        });
        if let Some((segmenter, segments)) = &mut self.segmenter
            && let Err(error) =
                segmenter.push(&self.converted, &mut |segment| segments.emit(segment))
        {
            self.fail(error);
        }
    }

    fn fail(&mut self, error: PortError) {
        tracing::error!(
            code = error.error().code().as_str(),
            detail = error.detail(),
            "capture worker failed"
        );
        self.events.emit(CaptureEvent::Failed(error.clone()));
        self.failure = Some(error);
    }

    /// Flushes the converter tail and the open segment, finalizes the journal and builds the outcome.
    fn finish(mut self) -> CaptureOutcome {
        if self.failure.is_none() {
            self.converted.clear();
            match self.converter.finish(&mut self.converted) {
                Ok(()) => self.consume_converted(),
                Err(error) => self.fail(error),
            }
        }
        let failed = self.failure.is_some();
        let (vad, speech_ms, segments) = match self.segmenter.take() {
            Some((segmenter, sink)) => {
                let totals = segmenter.finish(&mut |segment| {
                    if !failed {
                        sink.emit(segment);
                    }
                });
                // After a detector failure its state is unknown, so it is not handed back.
                (
                    (!failed).then_some(totals.vad),
                    totals.speech_ms,
                    totals.segments,
                )
            }
            None => (None, 0, 0),
        };
        if let Some(journal) = self.journal.take()
            && let Err(error) = journal.finalize()
            && self.failure.is_none()
        {
            self.fail(error);
        }
        let channels = u64::from(self.format.channels.max(1));
        let rate = u64::from(self.format.sample_rate.max(1));
        CaptureOutcome {
            summary: CaptureSummary {
                duration_ms: samples_to_ms(self.samples_out),
                speech_ms,
                segments,
                peak_rms: self.meter.peak_rms(),
                mean_rms: self.meter.mean_rms(),
                dropped_ms: self.dropped.load(Ordering::Relaxed) / channels * 1000 / rate,
            },
            vad,
            failure: self.failure,
        }
    }
}

#[cfg(test)]
mod tests {
    use hound::WavReader;

    use super::*;
    use crate::{
        ports::fakes::{FakeAudioCapture, FakeVoiceActivity, FakeWorkerScheduler, RecordingSink},
        types::{Permission, StaticList, testing::TempDir},
    };

    const STEREO_48K: CaptureFormat = CaptureFormat {
        sample_rate: 48_000,
        channels: 2,
    };

    /// `ms` of 48 kHz stereo audio at a constant `level`.
    fn audio(ms: usize, level: f32) -> Vec<f32> {
        vec![level; 48 * ms * 2]
    }

    struct Take {
        capture: FakeAudioCapture,
        levels: Arc<RecordingSink<AppEvent>>,
        events: Arc<RecordingSink<CaptureEvent>>,
        segments: Arc<RecordingSink<SpeechSegment>>,
        scheduler: Arc<FakeWorkerScheduler>,
        vad: FakeVoiceActivity,
        dir: TempDir,
    }

    impl Take {
        fn new() -> Self {
            Self {
                capture: FakeAudioCapture::new(STEREO_48K),
                levels: Arc::default(),
                events: Arc::default(),
                segments: Arc::default(),
                scheduler: Arc::default(),
                vad: FakeVoiceActivity::new(32),
                dir: TempDir::new("capture"),
            }
        }

        fn journal(&self) -> PathBuf {
            self.dir.join("recordings").join("take.wav")
        }

        fn config(&self) -> CaptureConfig {
            CaptureConfig {
                journal: Some(self.journal()),
                segmentation: Some(Segmentation {
                    vad: Box::new(self.vad.clone()),
                    policy: SegmentPolicy::DEFAULT,
                    segments: self.segments.clone(),
                }),
                levels: Some(self.levels.clone()),
                events: self.events.clone(),
                scheduler: self.scheduler.clone(),
            }
        }

        fn start(&self) -> Capture {
            Capture::start(&self.capture, None, self.config()).unwrap()
        }
    }

    #[test]
    fn a_take_is_journaled_levelled_and_segmented() {
        let take = Take::new();
        let capture = take.start();
        assert_eq!(capture.format(), STEREO_48K);
        // 1.7 s in total, inside the ring's 2 s, so the test never depends on how fast the worker drains.
        assert!(take.capture.feed(&audio(300, 0.0)));
        assert!(take.capture.feed(&audio(400, 0.3)));
        assert!(take.capture.feed(&audio(700, 0.0)));
        assert!(take.capture.feed(&audio(300, 0.3)));
        let outcome = capture.finish();

        assert!(outcome.failure.is_none());
        assert!(!take.capture.is_open(), "finish closes the microphone");
        assert_eq!(outcome.summary.duration_ms, 1_700);
        assert_eq!(outcome.summary.segments, 2);
        assert_eq!(outcome.summary.dropped_ms, 0);
        assert!((outcome.summary.peak_rms - 0.3).abs() < 0.01);
        assert!(
            outcome.vad.is_some(),
            "the detector comes back for the next take"
        );
        assert_eq!(take.vad.resets(), 1);

        let segments = take.segments.events();
        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.index)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert!(
            segments[0].start_ms < 300,
            "pre-roll starts the segment before the speech"
        );

        let reader = WavReader::open(take.journal()).unwrap();
        assert_eq!(reader.spec().sample_rate, 16_000);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.len(), 27_200);

        let levels = take.levels.events();
        assert_eq!(levels.len(), 27_200 / level::LEVEL_WINDOW_SAMPLES);
        assert!(
            levels
                .iter()
                .all(|event| matches!(event, AppEvent::AudioLevel(_)))
        );
        assert_eq!(
            take.scheduler.requests(),
            [(Some("echo-capture".to_owned()), WorkerPriority::AboveNormal)]
        );
    }

    #[test]
    fn nothing_flows_while_paused() {
        let take = Take::new();
        let mut capture = take.start();
        assert!(take.capture.feed(&audio(500, 0.2)));
        capture.pause().unwrap();
        assert!(!take.capture.feed(&audio(500, 0.2)));
        capture.resume().unwrap();
        assert!(take.capture.feed(&audio(500, 0.2)));
        let outcome = capture.finish();
        assert_eq!(outcome.summary.duration_ms, 1_000);
    }

    /// The device is unplugged mid-take and the session finishes the capture: nothing captured is lost.
    #[test]
    fn device_loss_keeps_a_valid_journal_of_what_was_captured() {
        let take = Take::new();
        let capture = take.start();
        assert!(take.capture.feed(&audio(1_500, 0.3)));
        take.capture.lose_device();
        assert_eq!(take.events.events(), [CaptureEvent::DeviceLost]);
        let outcome = capture.finish();
        assert!(outcome.failure.is_none());
        assert_eq!(outcome.summary.duration_ms, 1_500);
        assert_eq!(
            take.segments.events().len(),
            1,
            "the speech so far is still transcribed"
        );
        assert_eq!(WavReader::open(take.journal()).unwrap().len(), 24_000);
    }

    /// The owner drops the capture without finishing it (an error path): the journal is still finalized.
    #[test]
    fn dropping_an_open_capture_closes_the_mic_and_finalizes_the_journal() {
        let take = Take::new();
        let capture = take.start();
        assert!(take.capture.feed(&audio(250, 0.1)));
        drop(capture);
        assert!(!take.capture.is_open());
        assert_eq!(WavReader::open(take.journal()).unwrap().len(), 4_000);
    }

    #[test]
    fn a_detector_failure_is_reported_once_and_the_journal_is_kept() {
        struct Broken;
        impl VoiceActivity for Broken {
            fn caps(&self) -> crate::types::VadCaps {
                crate::types::VadCaps { frame_ms: 32 }
            }
            fn reset(&mut self) -> PortResult<()> {
                Ok(())
            }
            fn push(&mut self, _: &[f32]) -> PortResult<crate::types::VadEvent> {
                Err(PortError::new(AppError::Internal).with_detail("inference failed"))
            }
        }
        let take = Take::new();
        let mut config = take.config();
        config.segmentation = Some(Segmentation {
            vad: Box::new(Broken),
            policy: SegmentPolicy::DEFAULT,
            segments: take.segments.clone(),
        });
        let capture = Capture::start(&take.capture, None, config).unwrap();
        assert!(take.capture.feed(&audio(500, 0.3)));
        assert!(take.capture.feed(&audio(500, 0.3)));
        let outcome = capture.finish();
        assert_eq!(
            outcome.failure.map(PortError::into_app_error),
            Some(AppError::Internal)
        );
        assert!(outcome.vad.is_none());
        assert_eq!(take.events.events().len(), 1);
        assert!(WavReader::open(take.journal()).unwrap().len() > 0);
    }

    #[test]
    fn a_failed_open_leaves_no_stream_and_keeps_the_ports_error() {
        let take = Take::new();
        take.capture.fail_next_start(
            AppError::PermissionDenied {
                permission: Permission::Microphone,
            }
            .into(),
        );
        let error = Capture::start(&take.capture, None, take.config())
            .err()
            .map(PortError::into_app_error);
        assert_eq!(
            error,
            Some(AppError::PermissionDenied {
                permission: Permission::Microphone
            })
        );
        assert!(!take.capture.is_open());
    }

    #[test]
    fn a_ring_that_cannot_keep_up_drops_whole_buffers_and_counts_them() {
        let (producer, mut consumer) = RingBuffer::new(4);
        let dropped = Arc::new(AtomicU64::new(0));
        let mut sink = RingSink {
            producer,
            dropped: Arc::clone(&dropped),
        };
        sink.push(&[0.1, 0.2]);
        sink.push(&[0.3, 0.4, 0.5]);
        assert_eq!(dropped.load(Ordering::Relaxed), 3);
        assert_eq!(consumer.slots(), 2);
        assert_eq!(consumer.pop(), Ok(0.1));
    }

    #[test]
    fn the_ring_holds_two_seconds_of_the_largest_declared_format() {
        let caps = AudioCaps {
            sample_rates: StaticList::from(vec![16_000, 48_000]),
            channels: StaticList::from(vec![1, 2]),
        };
        assert_eq!(ring_capacity(&caps), 192_000);
        let huge = AudioCaps {
            sample_rates: StaticList::from(vec![384_000]),
            channels: StaticList::from(vec![32]),
        };
        assert_eq!(ring_capacity(&huge), RING_MAX_SAMPLES);
    }
}
