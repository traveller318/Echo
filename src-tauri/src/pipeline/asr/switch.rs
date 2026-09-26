/*!
 * SOURCE OF TRUTH KEYWORDS: engine switch, reload_on_change, remeasure, transcription.engine change, accelerator change, live engine swap, forget accelerator measurement
 * WHAT:  `reload_on_change`: after a settings write, asks the ASR worker to load the engine the new settings select
 *        when the load request (engine, model folder, accelerator) differs from the one the old settings gave.
 *        `remeasure`: forgets the selected engine's remembered accelerator measurements and, when it runs on `auto`
 *        and is loaded, reloads it so the measurement runs now.
 * WHY:   A new engine or accelerator takes effect without a restart (02 §8.1): the worker builds and warms the new
 *        engine on its loader thread while the current one keeps serving takes, then swaps; a take in progress stays
 *        on its engine. Requests are compared after caps narrowing (`load_request`), so a preference the engine
 *        cannot honour changes nothing. While nothing has been loaded yet (Unloaded, before the startup load or with
 *        the lazy load of step 25) nothing is requested, because that load reads the settings in effect anyway. A
 *        request that cannot be built (no ASR engine selected) is only logged: the next take reports it. A
 *        re-measurement forgets every GPU's answer for the engine (a stale one would come back with that GPU) and
 *        reloads only for `auto`, the only request a measurement decides; an engine that is not loaded, or whose
 *        model is missing, measures at its next load anyway.
 * WHERE: pipeline/settings_effects.rs after `settings_set` / `settings_reset`; models_set_active (step 21) writes
 *        `transcription.engine` and reaches the swap through the same path; `remeasure` by `engine_remeasure`
 *        (ipc/commands/engine.rs).
 */

use super::{AsrWorker, load_request};
use crate::{
    services::{Db, accelerator_benchmarks},
    types::{
        AcceleratorRequest, AppError, AppPaths, AsrLoadRequest, AsrReadiness, PortResult,
        SettingsSnapshot,
    },
};

/// Requests the engine `after` selects when its load request differs from `before`'s; true when a load was asked.
pub fn reload_on_change(
    asr: &AsrWorker,
    before: &SettingsSnapshot,
    after: &SettingsSnapshot,
    paths: &AppPaths,
) -> bool {
    let wanted = match load_request(after, paths) {
        Ok(request) => request,
        Err(error) => {
            tracing::warn!(
                code = error.error().code().as_str(),
                detail = error.detail(),
                "the new speech engine settings name no loadable engine"
            );
            return false;
        }
    };
    if load_request(before, paths).ok().as_ref() == Some(&wanted) {
        return false;
    }
    switch_to(asr, wanted)
}

/// Forgets the selected engine's accelerator measurements; reloads it (measuring now) when it is loaded on `auto`.
/// True when a reload was asked.
pub fn remeasure(
    asr: &AsrWorker,
    db: &Db,
    settings: &SettingsSnapshot,
    paths: &AppPaths,
) -> PortResult<bool> {
    let request = load_request(settings, paths)?;
    let forgotten = accelerator_benchmarks::clear::for_engine(db, &request.engine_id)?;
    tracing::info!(engine = %request.engine_id, forgotten, "accelerator measurements forgotten");
    let loadable = !matches!(
        asr.readiness(),
        AsrReadiness::Failed {
            error: AppError::ModelMissing { .. },
            ..
        }
    );
    Ok(request.accelerator == AcceleratorRequest::Auto && loadable && switch_to(asr, request))
}

/// Asks the worker to load `request` unless nothing was ever loaded; true when a load was asked.
fn switch_to(asr: &AsrWorker, request: AsrLoadRequest) -> bool {
    if asr.readiness() == AsrReadiness::Unloaded {
        return false;
    }
    tracing::info!(engine = %request.engine_id, accelerator = ?request.accelerator, "switching the speech engine");
    // The outcome reaches takes through the worker's readiness; nobody waits on it here.
    drop(asr.load(request));
    true
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
        registry::settings::{defaults, keys, resolve, values},
        types::{Accelerator, AcceleratorRequest, EngineId, ModelId, SettingValue, StaticStr},
    };

    fn worker() -> AsrWorker {
        AsrWorker::spawn(AsrWorkerConfig {
            accelerators: crate::pipeline::asr::AcceleratorPicker::without_gpu(),
            build: Arc::new(|_: &EngineId| {
                Ok(Arc::new(FakeAsrEngine::english()) as Arc<dyn AsrEngine>)
            }),
            scheduler: Arc::new(FakeWorkerScheduler::default()),
            readiness: None,
        })
        .unwrap()
    }

    fn paths() -> AppPaths {
        let root = std::env::temp_dir().join("echo-engine-switch");
        AppPaths::new(root.join("data"), root.join("resources"))
    }

    fn request(engine: &'static str) -> AsrLoadRequest {
        AsrLoadRequest {
            engine_id: EngineId::from_static(engine),
            model_dir: paths().model_dir(&ModelId::from_static(engine)),
            accelerator: AcceleratorRequest::Fixed(Accelerator::Cpu),
        }
    }

    #[test]
    fn an_unrelated_write_or_a_change_before_the_first_load_loads_nothing() {
        let asr = worker();
        let before = defaults();
        let unrelated = resolve([(keys::AUTO_PASTE, SettingValue::Bool(false))]);
        assert!(!reload_on_change(&asr, &before, &unrelated, &paths()));
        // Auto → GPU is a different request, but nothing was loaded yet: the startup load reads the new settings.
        let gpu = resolve([(
            keys::ACCELERATOR,
            SettingValue::Enum(StaticStr::new(values::GPU)),
        )]);
        assert_ne!(
            load_request(&before, &paths()).unwrap(),
            load_request(&gpu, &paths()).unwrap()
        );
        assert!(!reload_on_change(&asr, &before, &gpu, &paths()));
        assert_eq!(asr.readiness(), AsrReadiness::Unloaded);
    }

    #[test]
    fn a_switch_waits_for_a_first_load_then_swaps_the_engine() {
        let asr = worker();
        assert!(
            !switch_to(&asr, request("engine-a")),
            "nothing loaded yet: the startup load reads the new settings"
        );
        asr.load(request("engine-a"))
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert!(switch_to(&asr, request("engine-b")));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let ready_b = |readiness: &AsrReadiness| matches!(readiness, AsrReadiness::Ready { engine_id, .. } if engine_id.as_str() == "engine-b");
        while !ready_b(&asr.readiness()) {
            assert!(
                std::time::Instant::now() < deadline,
                "the new engine never became ready"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn remeasure_forgets_the_engine_s_answers_and_reloads_only_a_loaded_auto_engine() {
        use crate::{
            registry::engines::PARAKEET_TDT_V3,
            types::{AcceleratorBenchmark, UnixMs},
        };

        let asr = worker();
        let db = Db::open_in_memory().unwrap();
        let remembered = AcceleratorBenchmark {
            chosen: Accelerator::Cpu,
            timings: Vec::new(),
            measured_at: UnixMs::from_millis(1),
        };
        accelerator_benchmarks::put::put(&db, &PARAKEET_TDT_V3, "gpu", &remembered).unwrap();
        assert!(
            !remeasure(&asr, &db, &defaults(), &paths()).unwrap(),
            "not loaded yet: the first load measures"
        );
        assert_eq!(
            accelerator_benchmarks::get::one(&db, &PARAKEET_TDT_V3, "gpu").unwrap(),
            None
        );
        asr.load(load_request(&defaults(), &paths()).unwrap())
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert!(remeasure(&asr, &db, &defaults(), &paths()).unwrap());
        let cpu = resolve([(
            keys::ACCELERATOR,
            SettingValue::Enum(StaticStr::new(values::CPU)),
        )]);
        assert!(
            !remeasure(&asr, &db, &cpu, &paths()).unwrap(),
            "a fixed accelerator has nothing to measure"
        );
    }
}
