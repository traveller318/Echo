/*!
 * SOURCE OF TRUTH KEYWORDS: AsrOutput, AsrEvent, AsrReadiness, AsrLoadRequest, SpeechEngineStatus, SegmentDone, segment text, engine readiness, engine load request
 * WHAT:  The speech recognition shapes: what an engine returns for one segment (AsrOutput), what the ASR worker
 *        reports per take (AsrEvent: SegmentDone / SegmentFailed / Drained), whether a take started now could be
 *        transcribed (AsrReadiness), which engine to load and how (AsrLoadRequest), and the status the UI reads
 *        (SpeechEngineStatus: readiness plus where the engine runs and why, types/accelerator.rs).
 * WHY:   Every engine (Parakeet today, Whisper or Moonshine later, 02 §8.4) returns one AsrOutput shape, so the ASR
 *        worker and the segment join never know which engine ran. The detected language is optional because only
 *        engines with `AsrCaps.auto_language` that also report it fill it; it is stored as `transcripts.language`.
 *        AsrEvent carries the take's TranscriptId so a late result from a discarded take can never land in the
 *        next one, and its segment index because text is joined by index (02 §6.1). `Drained` follows the last
 *        segment of a finished take, so the session knows every segment is in without counting. AsrReadiness
 *        derives specta so the Models page and About can show it once a command or event carries it; it describes
 *        the engine a take started *now* would use, so it stays `Ready` while a replacement engine warms up
 *        (02 §8.1). AsrLoadRequest is resolved from settings and the registry by the pipeline and built into an
 *        engine by the registry, so the request itself names no model. Its accelerator is a request (auto or one
 *        accelerator) that the pipeline's accelerator picker turns into a device when the load runs, because GPUs
 *        and drivers can change between the request and the load.
 * WHERE: AsrOutput is returned by `AsrEngine::transcribe` (ports/asr.rs); pipeline/asr (the ASR worker) emits
 *        AsrEvent, tracks AsrReadiness and takes AsrLoadRequest from app/bootstrap (startup load) and the engine
 *        switch; the session actor consumes AsrEvent; SpeechEngineStatus is returned by `engine_status`
 *        (ipc/commands/engine.rs).
 */

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{
    Accelerator, AcceleratorChoice, AcceleratorRequest, AppError, EngineId, Language, PortError,
    TranscriptId,
};

/// Text an ASR engine produced for one segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrOutput {
    /// Recognized text, trimmed; empty when the segment held no words.
    pub text: String,
    /// The language the engine detected, when it reports one.
    pub language: Option<Language>,
}

/// What the ASR worker reports about one take's segments, in the order it handled them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsrEvent {
    /// Segment `index` of `take` was transcribed.
    SegmentDone {
        take: TranscriptId,
        index: u32,
        output: AsrOutput,
    },
    /// Segment `index` of `take` could not be transcribed (no engine, model missing, inference error).
    SegmentFailed {
        take: TranscriptId,
        index: u32,
        error: PortError,
    },
    /// Every segment of `take` handed over before the take was finished has been reported.
    Drained { take: TranscriptId },
}

impl AsrEvent {
    /// The take this event belongs to.
    pub const fn take(&self) -> TranscriptId {
        match self {
            Self::SegmentDone { take, .. }
            | Self::SegmentFailed { take, .. }
            | Self::Drained { take } => *take,
        }
    }
}

/// Whether a take started now could be transcribed, and by which engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AsrReadiness {
    /// No engine is loaded and none is loading.
    Unloaded,
    /// `engine_id` is loading and warming up; segments wait for it.
    Loading { engine_id: EngineId },
    /// `engine_id` is loaded, warm and running on `accelerator`.
    Ready {
        engine_id: EngineId,
        accelerator: Accelerator,
    },
    /// `engine_id` could not be loaded (e.g. `ModelMissing`); segments fail with this error.
    Failed {
        engine_id: EngineId,
        error: AppError,
    },
}

impl AsrReadiness {
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

/// Which engine to load, from where, on which accelerator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrLoadRequest {
    /// A registry engine of kind Asr.
    pub engine_id: EngineId,
    /// The folder holding the engine's model files (`AppPaths::model_dir`).
    pub model_dir: PathBuf,
    /// Auto (measure and keep the faster) or one of the engine's declared accelerators.
    pub accelerator: AcceleratorRequest,
}

/// What the speech engine is doing and, once one is ready, where it runs and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SpeechEngineStatus {
    pub readiness: AsrReadiness,
    /// Where the ready engine runs; None while no engine is ready.
    pub accelerator: Option<AcceleratorChoice>,
}

impl SpeechEngineStatus {
    pub const UNLOADED: Self = Self {
        readiness: AsrReadiness::Unloaded,
        accelerator: None,
    };
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::types::ModelId;

    #[test]
    fn readiness_is_tagged_by_kind() {
        let ready = AsrReadiness::Ready {
            engine_id: EngineId::from_static("parakeet-tdt-0.6b-v3"),
            accelerator: Accelerator::Cpu,
        };
        assert!(ready.is_ready());
        assert_eq!(
            serde_json::to_value(&ready).unwrap(),
            json!({ "kind": "ready", "engine_id": "parakeet-tdt-0.6b-v3", "accelerator": "cpu" })
        );
        let failed = AsrReadiness::Failed {
            engine_id: EngineId::from_static("parakeet-tdt-0.6b-v3"),
            error: AppError::ModelMissing {
                model_id: ModelId::from_static("parakeet-tdt-0.6b-v3"),
            },
        };
        assert!(!failed.is_ready());
        assert_eq!(
            serde_json::to_value(&failed).unwrap()["error"]["code"],
            json!("ModelMissing")
        );
        assert_eq!(
            serde_json::to_value(AsrReadiness::Unloaded).unwrap(),
            json!({ "kind": "unloaded" })
        );
    }

    #[test]
    fn every_event_names_its_take() {
        let take = TranscriptId::generate();
        let events = [
            AsrEvent::SegmentDone {
                take,
                index: 0,
                output: AsrOutput {
                    text: String::from("Hello."),
                    language: None,
                },
            },
            AsrEvent::SegmentFailed {
                take,
                index: 1,
                error: PortError::new(AppError::Asr),
            },
            AsrEvent::Drained { take },
        ];
        assert!(events.iter().all(|event| event.take() == take));
    }
}
