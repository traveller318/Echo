/*!
 * SOURCE OF TRUTH KEYWORDS: speech engine check, EngineCheck, usable_engine, readiness model check, reload failed engine, ModelMissing, segment_policy, engine segment limit
 * WHAT:  `usable_engine`: decides from the ASR worker's readiness whether a take (live or a retry) can be
 *        transcribed now, and on which engine; `segment_policy`: the segmentation rules for that engine.
 * WHY:   A live take and a retry must agree on when speech can be transcribed, so the rule lives once here. The
 *        check reads readiness instead of the disk: `Ready`/`Loading` are usable (segments wait for a load), a
 *        missing model answers ModelMissing at once (installing the model is what loads it, the model manager, so
 *        a press never retries a load that cannot succeed), and any other failed or absent load is requested
 *        again in the background, so a transient failure never needs a restart. A load that cannot even be
 *        requested (no ASR engine selected) is the caller's error. Segments are cut no longer than the engine
 *        accepts in one call (SegmentPolicy::within_engine_limit), from its registry caps.
 * WHERE: pipeline/session/arm.rs (Arm effect) and pipeline/retry.rs (session_retry).
 */

use super::{AsrWorker, load_request};
use crate::{
    registry,
    types::{
        AppError, AppPaths, AsrReadiness, EngineId, ModelId, PortResult, SegmentPolicy,
        SettingsSnapshot,
    },
};

/// What the speech engine check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineCheck {
    /// Speech can be transcribed: this engine is ready or loading (segments wait for the load).
    Usable(EngineId),
    /// The selected engine's model is not installed.
    ModelMissing(ModelId),
}

/// Whether speech handed to the ASR worker now would be transcribed, and by which engine.
pub fn usable_engine(
    asr: &AsrWorker,
    settings: &SettingsSnapshot,
    paths: &AppPaths,
) -> PortResult<EngineCheck> {
    match asr.readiness() {
        AsrReadiness::Ready { engine_id, .. } | AsrReadiness::Loading { engine_id } => {
            Ok(EngineCheck::Usable(engine_id))
        }
        AsrReadiness::Failed {
            error: AppError::ModelMissing { model_id },
            ..
        } => Ok(EngineCheck::ModelMissing(model_id)),
        AsrReadiness::Failed { .. } | AsrReadiness::Unloaded => {
            request_load(asr, settings, paths).map(EngineCheck::Usable)
        }
    }
}

/// Asks the ASR worker to load the engine the settings select; returns its id at once. Also used by the model
/// manager after the selected engine's model is installed or removed.
pub fn request_load(
    asr: &AsrWorker,
    settings: &SettingsSnapshot,
    paths: &AppPaths,
) -> PortResult<EngineId> {
    let request = load_request(settings, paths)?;
    let engine = request.engine_id.clone();
    tracing::info!(%engine, "loading the speech engine on demand");
    // The outcome reaches the caller through the worker: its segments wait for the load, or fail with its error.
    drop(asr.load(request));
    Ok(engine)
}

/// The segmentation rules for speech transcribed on `engine`: the defaults, cut no longer than it accepts.
pub fn segment_policy(engine: &EngineId) -> SegmentPolicy {
    registry::engines::find(engine)
        .and_then(|entry| entry.asr_caps())
        .map_or(SegmentPolicy::DEFAULT, |caps| {
            SegmentPolicy::DEFAULT.within_engine_limit(caps.max_segment_s)
        })
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::*;
    use crate::{
        pipeline::asr::AsrWorkerConfig,
        ports::{
            AsrEngine,
            fakes::{FakeAsrEngine, FakeWorkerScheduler},
        },
        registry::{engines::PARAKEET_TDT_V3, settings::defaults},
        types::PortError,
    };

    fn worker(engine: impl Fn() -> FakeAsrEngine + Send + Sync + 'static) -> AsrWorker {
        AsrWorker::spawn(AsrWorkerConfig {
            accelerators: crate::pipeline::asr::AcceleratorPicker::without_gpu(),
            build: Arc::new(move |_: &EngineId| Ok(Arc::new(engine()) as Arc<dyn AsrEngine>)),
            scheduler: Arc::new(FakeWorkerScheduler::default()),
            readiness: None,
        })
        .unwrap()
    }

    fn paths() -> AppPaths {
        let root = std::env::temp_dir().join("echo-engine-check");
        AppPaths::new(root.join("data"), root.join("resources"))
    }

    #[test]
    fn an_unloaded_worker_is_asked_to_load_the_selected_engine() {
        let asr = worker(FakeAsrEngine::english);
        let check = usable_engine(&asr, &defaults(), &paths()).unwrap();
        assert_eq!(check, EngineCheck::Usable(PARAKEET_TDT_V3));
        // The worker applies the requested load on its own thread.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !matches!(asr.readiness(), AsrReadiness::Ready { .. }) {
            assert!(
                std::time::Instant::now() < deadline,
                "the load never finished"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_ready_engine_is_used_as_is() {
        let asr = worker(FakeAsrEngine::english);
        let request = load_request(&defaults(), &paths()).unwrap();
        asr.load(request)
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(
            usable_engine(&asr, &defaults(), &paths()).unwrap(),
            EngineCheck::Usable(PARAKEET_TDT_V3)
        );
    }

    #[test]
    fn a_missing_model_is_reported_without_another_load() {
        let model_id = ModelId::from_static("parakeet-tdt-0.6b-v3");
        let missing = model_id.clone();
        let asr = worker(move || {
            let engine = FakeAsrEngine::english();
            engine.fail_next_load(PortError::new(AppError::ModelMissing {
                model_id: missing.clone(),
            }));
            engine
        });
        let request = load_request(&defaults(), &paths()).unwrap();
        assert!(
            asr.load(request)
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .is_err()
        );
        assert_eq!(
            usable_engine(&asr, &defaults(), &paths()).unwrap(),
            EngineCheck::ModelMissing(model_id)
        );
        assert!(matches!(asr.readiness(), AsrReadiness::Failed { .. }));
    }

    #[test]
    fn segments_are_cut_within_the_engine_limit() {
        let policy = segment_policy(&PARAKEET_TDT_V3);
        assert!(policy.max_segment_ms <= SegmentPolicy::DEFAULT.max_segment_ms);
        assert_eq!(
            segment_policy(&EngineId::from_static("no-such-engine")),
            SegmentPolicy::DEFAULT
        );
    }
}
