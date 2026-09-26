/*!
 * SOURCE OF TRUTH KEYWORDS: arm take, ArmRequest, ArmOutcome, OpenTake, row before mic, model check, open microphone, pinned device fallback, speech engine readiness
 * WHAT:  `arm`: the Arm effect of 02 §5, run off the actor on a blocking thread. In order: read the target window
 *        under the take's TargetRule (pipeline/delivery.rs `find_target`), check that the speech engine can transcribe, build the voice detector, insert the take's row as
 *        `recording`, open the ASR take, then open the microphone with the journal, levels and segmentation.
 *        Returns what it opened (Armed), that the model is not installed (ModelMissing, nothing touched), or why
 *        it failed and whether the row exists (Failed).
 * WHY:   02 §7.3 step 1: the row exists before the microphone produces audio, so a crash from here on leaves a row
 *        startup recovery can find. Everything that can fail on its own and needs no row (the model, the detector)
 *        is checked first, so those failures leave nothing behind. Opening a WASAPI stream and a Silero detector
 *        take tens of milliseconds and must not hold up the actor's inbox (a stop during arming is remembered, not
 *        queued behind it). The model check reads the ASR worker's readiness instead of the disk: a load in
 *        progress is fine (segments wait for it), a missing model answers ModelMissing, and any other failed or
 *        absent load is requested again in the background (its segments wait for it, or fail with its error and
 *        keep the audio); a load that cannot even be requested (no ASR engine selected) fails the take. The
 *        target is read before the pill shows (05 W3); an unreadable foreground only means the text is copied.
 *        A pinned microphone that is gone falls back to the Windows default (05 W12: the next take uses the new
 *        default) instead of failing the take. Segments are cut by `asr::segment_policy` (the engine's limit).
 * WHERE: Spawned by the runner (runner.rs) for SessionEffect::Arm; its ArmOutcome comes back as WorkerReply::Arm.
 */

use std::sync::Arc;

use super::inbox::{Outbox, TakeEvents};
use crate::{
    pipeline::{
        asr::{self, AsrTake, AsrWorker, EngineCheck},
        capture::{Capture, CaptureConfig, Segmentation},
        delivery,
    },
    ports::{AudioCapture, EventSink, ForegroundApp, VoiceActivity, WorkerScheduler},
    registry,
    services::{self, Db},
    types::{
        AppError, AppEvent, AppPaths, AppTarget, AudioDeviceId, EngineId, HistoryChangeReason,
        HistoryChanged, ModelId, NewTranscript, PortError, PortResult, ResourceKind, SegmentPolicy,
        SettingsSnapshot, TargetRule, TranscriptId, TranscriptStatus, UnixMs,
    },
};

/// Builds a fresh voice activity detector.
pub type VadBuilder = Arc<dyn Fn() -> PortResult<Box<dyn VoiceActivity>> + Send + Sync>;

/// Everything one Arm needs, moved onto its blocking thread.
pub(super) struct ArmRequest {
    pub take: TranscriptId,
    /// Which window the take's text goes to.
    pub target: TargetRule,
    pub settings: Arc<SettingsSnapshot>,
    /// A detector left by the previous take or built ahead; None builds one.
    pub detector: Option<Box<dyn VoiceActivity>>,
    pub build_detector: VadBuilder,
    pub foreground: Arc<dyn ForegroundApp>,
    pub audio: Arc<dyn AudioCapture>,
    pub scheduler: Arc<dyn WorkerScheduler>,
    pub asr: AsrWorker,
    pub db: Db,
    pub paths: AppPaths,
    pub events: Arc<dyn EventSink<AppEvent>>,
    pub outbox: Outbox,
}

/// What a take holds from the moment its microphone is open.
pub(super) struct OpenTake {
    /// The open microphone; `capture.transport()` says how it is connected.
    pub capture: Capture,
    pub asr: Arc<AsrTake>,
    /// The engine the take was started for (its row's `engine_id`).
    pub engine: EngineId,
}

/// How an Arm ended.
pub(super) enum ArmOutcome {
    Armed {
        take: TranscriptId,
        target: Option<AppTarget>,
        open: OpenTake,
    },
    /// The engine's model is not installed; nothing was written or opened.
    ModelMissing {
        take: TranscriptId,
        model_id: ModelId,
    },
    /// The take could not start; `row` says whether its row was inserted (and so must be marked failed).
    Failed {
        take: TranscriptId,
        error: PortError,
        row: bool,
    },
}

impl ArmOutcome {
    pub const fn take(&self) -> TranscriptId {
        match self {
            Self::Armed { take, .. }
            | Self::ModelMissing { take, .. }
            | Self::Failed { take, .. } => *take,
        }
    }
}

/// Runs the Arm effect for `request.take`; blocks for as long as the microphone takes to open.
pub(super) fn arm(mut request: ArmRequest) -> ArmOutcome {
    let take = request.take;
    let failed = |error: PortError, row: bool| ArmOutcome::Failed { take, error, row };

    let target = delivery::find_target(request.foreground.as_ref(), request.target);
    let engine = match asr::usable_engine(&request.asr, &request.settings, &request.paths) {
        Ok(EngineCheck::Usable(engine)) => engine,
        Ok(EngineCheck::ModelMissing(model_id)) => {
            return ArmOutcome::ModelMissing { take, model_id };
        }
        Err(error) => return failed(error, false),
    };
    let detector = match request.detector.take() {
        Some(detector) => detector,
        None => match (request.build_detector)() {
            Ok(detector) => detector,
            Err(error) => return failed(error, false),
        },
    };

    let row = NewTranscript {
        id: take,
        created_at: UnixMs::now(),
        status: TranscriptStatus::Recording,
        audio_path: Some(AppPaths::recording_name(take)),
        engine_id: Some(engine.clone()),
        app_name: target.as_ref().and_then(|target| target.exe_name.clone()),
    };
    if let Err(error) = services::transcripts::insert::insert(&request.db, &row) {
        return failed(error, false);
    }
    request.events.emit(
        HistoryChanged {
            reason: HistoryChangeReason::Inserted,
        }
        .into(),
    );

    let sink = Arc::new(TakeEvents {
        take,
        outbox: request.outbox.clone(),
    });
    let asr_take = request.asr.begin_take(
        take,
        registry::settings::language_preference(&request.settings),
        Arc::clone(&sink) as _,
    );
    let opener = Opener {
        request: &request,
        policy: asr::segment_policy(&engine),
        asr_take: &asr_take,
        sink: &sink,
    };
    match opener.open(detector) {
        Ok(capture) => ArmOutcome::Armed {
            take,
            target,
            open: OpenTake {
                capture,
                asr: asr_take,
                engine,
            },
        },
        // The ASR take is dropped with the outcome, which releases it on the worker.
        Err(error) => failed(error, true),
    }
}

/// Opens the take's microphone.
struct Opener<'a> {
    request: &'a ArmRequest,
    policy: SegmentPolicy,
    asr_take: &'a Arc<AsrTake>,
    sink: &'a Arc<TakeEvents>,
}

impl Opener<'_> {
    /// Opens the pinned microphone, or the Windows default when none is pinned or the pinned one is gone.
    fn open(&self, detector: Box<dyn VoiceActivity>) -> PortResult<Capture> {
        let pinned = registry::settings::input_device(&self.request.settings);
        let Some(device) = pinned else {
            return self.start(None, detector);
        };
        match self.start(Some(&device), detector) {
            Err(error)
                if error.error()
                    == &(AppError::NotFound {
                        resource: ResourceKind::AudioDevice,
                    }) =>
            {
                tracing::warn!(
                    detail = error.detail(),
                    "the chosen microphone is gone; recording from the Windows default"
                );
                // The failed start consumed the detector with its config.
                self.start(None, (self.request.build_detector)()?)
            }
            result => result,
        }
    }

    fn start(
        &self,
        device: Option<&AudioDeviceId>,
        detector: Box<dyn VoiceActivity>,
    ) -> PortResult<Capture> {
        let config = CaptureConfig {
            journal: Some(self.request.paths.recording(self.request.take)),
            segmentation: Some(Segmentation {
                vad: detector,
                policy: self.policy,
                segments: Arc::clone(self.asr_take) as _,
            }),
            levels: Some(Arc::clone(&self.request.events)),
            events: Arc::clone(self.sink) as _,
            scheduler: Arc::clone(&self.request.scheduler),
        };
        Capture::start(self.request.audio.as_ref(), device, config)
    }
}
