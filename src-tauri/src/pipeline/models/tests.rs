/*!
 * SOURCE OF TRUTH KEYWORDS: model manager tests, download retry test, cancel download test, import picker test, damaged model test, activation test, readiness relay test
 * WHAT:  ModelManager over FakeModelStore, FakeFolderPicker, a real ASR worker whose fake engines load only
 *        while the store says the model is installed, and a polish chain whose LLM stage is a fake: the list view,
 *        downloads (resume after a network drop, retries running out, cancel, Busy), import, verify and damage,
 *        removal, activation, the check after a failed load, install sets (the LLM and its runtime as one card),
 *        releasing the LLM stage before its files change and the download when grammar polish is switched on;
 *        ReadinessRelay's forwarding.
 * WHY:   02 §8.2's orchestration promises (progress ends with a terminal phase, a dropped connection costs no click,
 *        installing loads the engine, removing it makes the next press say "Model not installed") are pipeline
 *        behaviour, proven here without network or disk.
 * WHERE: `cargo test` (pipeline::models::tests).
 */

use std::{
    path::PathBuf,
    pin::pin,
    sync::Arc,
    time::{Duration, Instant},
};

use super::{ModelDeps, ModelManager, ModelPolicy, ModelWatch, ReadinessRelay};
use crate::{
    pipeline::{
        asr::{AsrWorker, AsrWorkerConfig},
        polish::PolishChains,
    },
    ports::{
        AsrEngine, EventSink, ModelStore, TextPolisher,
        fakes::{
            FakeAsrEngine, FakeFolderPicker, FakeModelStore, FakePolish, FakePrivacyConsent,
            FakeTextPolisher, FakeWorkerScheduler, RecordingSink, poll_once,
        },
    },
    registry::{
        engines::{PARAKEET_TDT_V3 as PARAKEET_ENGINE, QWEN3_POLISHER, SILERO_VAD},
        models::{self, LLAMA_CPP_VULKAN, PARAKEET_TDT_V3, QWEN3_1_7B_Q4, SILERO_VAD_V5},
        settings::{self, keys},
    },
    types::{
        AppError, AppEvent, AppPaths, AsrReadiness, EngineId, EngineRuntime, EngineSelection,
        ModelId, ModelPhase, ModelStatus, ModelTransferOutcome, PermissionState, PortError,
        ResourceKind, SettingValue, SharedSettings, StaticStr,
    },
};

const NO_WAIT: ModelPolicy = ModelPolicy {
    retry_delays: &[Duration::ZERO, Duration::ZERO],
    progress_interval: Duration::ZERO,
};

struct Rig {
    manager: ModelManager,
    store: Arc<FakeModelStore>,
    picker: Arc<FakeFolderPicker>,
    asr: AsrWorker,
    settings: SharedSettings,
    events: Arc<RecordingSink<AppEvent>>,
    polish: PolishChains,
    /// The grammar polish stage the chain builds for the LLM entry.
    llm: Arc<FakeTextPolisher>,
}

impl Rig {
    fn new() -> Self {
        Self::with_policy(NO_WAIT)
    }

    fn with_policy(policy: ModelPolicy) -> Self {
        let store = Arc::new(FakeModelStore::new("models"));
        let picker = Arc::new(FakeFolderPicker::default());
        let events = Arc::new(RecordingSink::default());
        let reading = Arc::clone(&store);
        let asr = AsrWorker::spawn(AsrWorkerConfig {
            accelerators: crate::pipeline::asr::AcceleratorPicker::without_gpu(),
            // Loads only while the store says Parakeet is installed, like the real adapter's file check.
            build: Arc::new(move |_: &EngineId| {
                let engine = FakeAsrEngine::english();
                let manifest = models::find(&PARAKEET_TDT_V3).unwrap();
                if !reading.status(manifest).unwrap().is_installed() {
                    engine.fail_next_load(
                        AppError::ModelMissing {
                            model_id: PARAKEET_TDT_V3,
                        }
                        .into(),
                    );
                }
                Ok(Arc::new(engine) as Arc<dyn AsrEngine>)
            }),
            scheduler: Arc::new(FakeWorkerScheduler::default()),
            readiness: None,
        })
        .unwrap();
        let settings = SharedSettings::new(settings::defaults());
        let root = std::env::temp_dir().join("echo-model-manager");
        let llm = Arc::new(FakeTextPolisher::slow(FakePolish::Map(str::to_owned)));
        let stage = Arc::clone(&llm);
        let polish = PolishChains::new(Arc::new(move |id: &EngineId| {
            Ok(if *id == QWEN3_POLISHER {
                Arc::clone(&stage) as Arc<dyn TextPolisher>
            } else {
                Arc::new(FakeTextPolisher::instant(FakePolish::Map(str::to_owned))) as _
            })
        }));
        let manager = ModelManager::new(
            ModelDeps {
                store: Arc::clone(&store) as _,
                picker: Arc::clone(&picker) as _,
                asr: asr.clone(),
                polish: polish.clone(),
                settings: settings.clone(),
                consent: Arc::new(FakePrivacyConsent::granted()),
                paths: AppPaths::new(root.join("data"), root.join("resources")),
                events: Arc::clone(&events) as _,
            },
            policy,
        );
        Self {
            manager,
            store,
            picker,
            asr,
            settings,
            events,
            polish,
            llm,
        }
    }

    /// The card of engine `id`.
    fn entry(&self, id: &EngineId) -> crate::types::ModelEntry {
        self.manager
            .list()
            .unwrap()
            .entries
            .into_iter()
            .find(|entry| entry.engine.id == *id)
            .unwrap()
    }

    /// The store's own status of one manifest.
    fn stored(&self, id: &ModelId) -> ModelStatus {
        self.store.status(models::find(id).unwrap()).unwrap()
    }

    /// Every ModelProgress phase emitted so far for `id`.
    fn phases(&self, id: &ModelId) -> Vec<ModelPhase> {
        self.events
            .events()
            .into_iter()
            .filter_map(|event| match event {
                AppEvent::ModelProgress(progress) if progress.model_id == *id => {
                    Some(progress.phase)
                }
                _ => None,
            })
            .collect()
    }

    fn models_changed(&self) -> usize {
        self.events
            .events()
            .iter()
            .filter(|event| matches!(event, AppEvent::ModelsChanged(_)))
            .count()
    }

    fn status(&self, id: &ModelId) -> ModelStatus {
        self.manager
            .list()
            .unwrap()
            .entries
            .into_iter()
            .find(|entry| entry.model.id == *id)
            .unwrap()
            .status
    }

    /// Waits until the ASR worker's readiness satisfies `done`.
    fn wait_for_readiness(&self, done: impl Fn(&AsrReadiness) -> bool) -> AsrReadiness {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let readiness = self.asr.readiness();
            if done(&readiness) {
                return readiness;
            }
            assert!(Instant::now() < deadline, "readiness stayed {readiness:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(future)
}

fn app_error<T: std::fmt::Debug>(result: Result<T, PortError>) -> AppError {
    result.unwrap_err().into_app_error()
}

#[test]
fn the_list_has_a_card_per_engine_with_a_model_and_follows_offline_mode() {
    let rig = Rig::new();
    let view = rig.manager.list().unwrap();
    let ids: Vec<&EngineId> = view.entries.iter().map(|entry| &entry.engine.id).collect();
    assert_eq!(ids, [&PARAKEET_ENGINE, &SILERO_VAD, &QWEN3_POLISHER]);
    assert_eq!(view.entries[0].status, ModelStatus::NotInstalled);
    assert_eq!(
        view.entries[0].selection,
        EngineSelection::Selectable { active: true }
    );
    assert_eq!(view.entries[1].status, ModelStatus::Installed);
    assert_eq!(view.entries[1].selection, EngineSelection::BuiltIn);
    assert!(view.entries.iter().all(|entry| entry.transfer.is_none()));
    assert_eq!(view.network, PermissionState::Granted);
    rig.settings.replace(settings::resolve([(
        keys::OFFLINE_MODE,
        SettingValue::Bool(true),
    )]));
    assert_eq!(rig.manager.list().unwrap().network, PermissionState::Denied);
}

#[test]
fn a_download_ends_ready_and_loads_the_selected_engine() {
    let rig = Rig::new();
    assert_eq!(
        block_on(rig.manager.download(&PARAKEET_TDT_V3)).unwrap(),
        ModelTransferOutcome::Completed
    );
    let phases = rig.phases(&PARAKEET_TDT_V3);
    assert_eq!(phases.first(), Some(&ModelPhase::Transferring));
    assert_eq!(phases.last(), Some(&ModelPhase::Ready));
    assert!(rig.models_changed() >= 1);
    assert_eq!(rig.status(&PARAKEET_TDT_V3), ModelStatus::Installed);
    rig.wait_for_readiness(AsrReadiness::is_ready);
    let view = rig.manager.list().unwrap();
    assert!(matches!(
        view.entries[0].runtime,
        Some(EngineRuntime::Ready {
            accelerator: Some(_)
        })
    ));

    // Installed already: nothing to do, nothing announced.
    let before = rig.events.events().len();
    assert_eq!(
        block_on(rig.manager.download(&PARAKEET_TDT_V3)).unwrap(),
        ModelTransferOutcome::Completed
    );
    assert_eq!(rig.events.events().len(), before);
}

#[test]
fn a_dropped_connection_waits_and_resumes_without_a_click() {
    let rig = Rig::new();
    rig.store.leave_partial(&PARAKEET_TDT_V3, 1_000);
    rig.store.fail_next(AppError::Network.into());
    assert_eq!(
        block_on(rig.manager.download(&PARAKEET_TDT_V3)).unwrap(),
        ModelTransferOutcome::Completed
    );
    let phases = rig.phases(&PARAKEET_TDT_V3);
    assert!(phases.contains(&ModelPhase::Waiting), "{phases:?}");
    assert_eq!(phases.last(), Some(&ModelPhase::Ready));
}

#[test]
fn when_the_waits_run_out_the_download_fails_and_keeps_its_partial() {
    let rig = Rig::with_policy(ModelPolicy {
        retry_delays: &[],
        ..NO_WAIT
    });
    rig.store.leave_partial(&PARAKEET_TDT_V3, 1_000);
    rig.store.fail_next(AppError::Network.into());
    assert_eq!(
        app_error(block_on(rig.manager.download(&PARAKEET_TDT_V3))),
        AppError::Network
    );
    assert_eq!(
        rig.phases(&PARAKEET_TDT_V3).last(),
        Some(&ModelPhase::Failed)
    );
    assert!(matches!(
        rig.status(&PARAKEET_TDT_V3),
        ModelStatus::Partial { .. }
    ));
}

#[test]
fn a_download_refused_by_offline_mode_is_not_retried() {
    let rig = Rig::new();
    rig.store.fail_next(
        AppError::PermissionDenied {
            permission: crate::types::Permission::Network,
        }
        .into(),
    );
    assert!(matches!(
        app_error(block_on(rig.manager.download(&PARAKEET_TDT_V3))),
        AppError::PermissionDenied { .. }
    ));
    assert!(!rig.phases(&PARAKEET_TDT_V3).contains(&ModelPhase::Waiting));
}

#[test]
fn cancel_stops_a_running_download_and_a_second_one_is_busy() {
    let rig = Rig::new();
    rig.store.leave_partial(&PARAKEET_TDT_V3, 500);
    rig.store.hang_transfers(true);
    let outcome = block_on(async {
        let id = PARAKEET_TDT_V3;
        let mut download = pin!(rig.manager.download(&id));
        assert!(poll_once(download.as_mut()).is_pending(), "the fake hangs");
        assert_eq!(
            app_error(rig.manager.download(&PARAKEET_TDT_V3).await),
            AppError::Busy
        );
        assert_eq!(
            app_error(rig.manager.remove(&PARAKEET_TDT_V3)),
            AppError::Busy
        );
        assert!(
            rig.manager.list().unwrap().entries[0].transfer.is_some(),
            "the page sees the running transfer"
        );
        assert!(rig.manager.cancel(&PARAKEET_TDT_V3));
        download.await.unwrap()
    });
    assert_eq!(outcome, ModelTransferOutcome::Cancelled);
    assert_eq!(
        rig.phases(&PARAKEET_TDT_V3).last(),
        Some(&ModelPhase::Cancelled)
    );
    assert!(matches!(
        rig.status(&PARAKEET_TDT_V3),
        ModelStatus::Partial { .. }
    ));
    assert!(
        !rig.manager.cancel(&PARAKEET_TDT_V3),
        "nothing runs any more"
    );
}

#[test]
fn import_asks_for_a_folder_and_a_closed_picker_is_cancelled() {
    let rig = Rig::new();
    assert_eq!(
        block_on(rig.manager.import(&PARAKEET_TDT_V3)).unwrap(),
        ModelTransferOutcome::Cancelled
    );
    assert!(rig.phases(&PARAKEET_TDT_V3).is_empty());
    assert_eq!(rig.status(&PARAKEET_TDT_V3), ModelStatus::NotInstalled);
    assert_eq!(
        rig.picker.titles(),
        ["Choose the folder with the Parakeet TDT 0.6B v3 files"]
    );

    rig.picker.answer(Some(PathBuf::from("picked")));
    assert_eq!(
        block_on(rig.manager.import(&PARAKEET_TDT_V3)).unwrap(),
        ModelTransferOutcome::Completed
    );
    assert_eq!(rig.status(&PARAKEET_TDT_V3), ModelStatus::Installed);
    assert_eq!(
        rig.phases(&PARAKEET_TDT_V3).last(),
        Some(&ModelPhase::Ready)
    );
}

#[test]
fn a_failed_check_marks_the_model_damaged_until_it_is_downloaded_again() {
    let rig = Rig::new();
    rig.store.install(&PARAKEET_TDT_V3);
    assert_eq!(
        block_on(rig.manager.verify(&PARAKEET_TDT_V3)).unwrap(),
        ModelTransferOutcome::Completed
    );
    rig.store.corrupt(&PARAKEET_TDT_V3);
    assert!(matches!(
        app_error(block_on(rig.manager.verify(&PARAKEET_TDT_V3))),
        AppError::ModelCorrupt { .. }
    ));
    assert_eq!(rig.status(&PARAKEET_TDT_V3), ModelStatus::Corrupt);
    assert!(matches!(
        app_error(rig.manager.activation(&PARAKEET_ENGINE)),
        AppError::ModelCorrupt { .. }
    ));

    // The sizes still look right, yet a download replaces the damaged install and clears the mark.
    rig.store.repair(&PARAKEET_TDT_V3);
    assert_eq!(
        block_on(rig.manager.download(&PARAKEET_TDT_V3)).unwrap(),
        ModelTransferOutcome::Completed
    );
    assert_eq!(rig.status(&PARAKEET_TDT_V3), ModelStatus::Installed);
    assert!(rig.manager.activation(&PARAKEET_ENGINE).is_ok());
}

#[test]
fn removing_the_selected_engines_model_makes_the_next_press_say_model_missing() {
    let rig = Rig::new();
    block_on(rig.manager.download(&PARAKEET_TDT_V3)).unwrap();
    rig.wait_for_readiness(AsrReadiness::is_ready);
    rig.manager.remove(&PARAKEET_TDT_V3).unwrap();
    assert_eq!(rig.status(&PARAKEET_TDT_V3), ModelStatus::NotInstalled);
    let readiness = rig.wait_for_readiness(|readiness| {
        matches!(
            readiness,
            AsrReadiness::Failed {
                error: AppError::ModelMissing { .. },
                ..
            }
        )
    });
    assert!(matches!(readiness, AsrReadiness::Failed { .. }));
    assert!(matches!(
        rig.manager.list().unwrap().entries[0].runtime,
        Some(EngineRuntime::Failed { .. })
    ));
    // Removing an absent model is fine; a bundled one is refused.
    rig.manager.remove(&PARAKEET_TDT_V3).unwrap();
    assert!(matches!(
        app_error(rig.manager.remove(&SILERO_VAD_V5)),
        AppError::Validation { .. }
    ));
}

#[test]
fn activation_needs_a_registered_selectable_engine_with_its_model() {
    let rig = Rig::new();
    assert!(matches!(
        app_error(rig.manager.activation(&PARAKEET_ENGINE)),
        AppError::ModelMissing { .. }
    ));
    rig.store.install(&PARAKEET_TDT_V3);
    assert_eq!(
        rig.manager.activation(&PARAKEET_ENGINE).unwrap(),
        [(
            keys::ASR_ENGINE,
            SettingValue::Enum(StaticStr::new("parakeet-tdt-0.6b-v3"))
        )]
    );
    assert!(matches!(
        app_error(rig.manager.activation(&SILERO_VAD)),
        AppError::Validation { .. }
    ));
    assert_eq!(
        app_error(rig.manager.activation(&EngineId::from_static("nope"))),
        AppError::NotFound {
            resource: ResourceKind::Engine
        }
    );
    assert_eq!(
        app_error(block_on(
            rig.manager.download(&ModelId::from_static("nope"))
        )),
        AppError::NotFound {
            resource: ResourceKind::Model
        }
    );
    assert!(matches!(
        app_error(block_on(rig.manager.download(&SILERO_VAD_V5))),
        AppError::Validation { .. }
    ));
}

#[test]
fn a_failed_load_checks_the_model_once_per_install() {
    let rig = Rig::new();
    rig.store.install(&PARAKEET_TDT_V3);
    rig.store.corrupt(&PARAKEET_TDT_V3);
    block_on(rig.manager.check_after_failed_load(&PARAKEET_ENGINE));
    assert_eq!(rig.status(&PARAKEET_TDT_V3), ModelStatus::Corrupt);
    let checks = rig.phases(&PARAKEET_TDT_V3).len();
    block_on(rig.manager.check_after_failed_load(&PARAKEET_ENGINE));
    assert_eq!(rig.phases(&PARAKEET_TDT_V3).len(), checks, "checked once");
    // Engines without a downloadable model are never checked.
    block_on(rig.manager.check_after_failed_load(&SILERO_VAD));
    block_on(
        rig.manager
            .check_after_failed_load(&EngineId::from_static("nope")),
    );
}

#[test]
fn the_readiness_relay_refreshes_the_page_and_queues_real_failures() {
    let rig = Rig::new();
    rig.store.install(&PARAKEET_TDT_V3);
    rig.store.corrupt(&PARAKEET_TDT_V3);
    let (relay, failures) = ReadinessRelay::new(Arc::clone(&rig.events) as _);
    relay.emit(AsrReadiness::Loading {
        engine_id: PARAKEET_ENGINE,
    });
    relay.emit(AsrReadiness::Failed {
        engine_id: PARAKEET_ENGINE,
        error: AppError::ModelMissing {
            model_id: PARAKEET_TDT_V3,
        },
    });
    relay.emit(AsrReadiness::Failed {
        engine_id: PARAKEET_ENGINE,
        error: AppError::Asr,
    });
    assert_eq!(rig.models_changed(), 3);
    drop(relay);
    block_on(ModelWatch::new(rig.manager.clone(), failures).run());
    assert_eq!(rig.status(&PARAKEET_TDT_V3), ModelStatus::Corrupt);
}

fn grammar_polish_on() -> crate::types::SettingsSnapshot {
    settings::resolve([(keys::LLM_ENABLED, SettingValue::Bool(true))])
}

#[test]
fn an_llm_and_its_runtime_are_one_card_with_one_download() {
    let rig = Rig::new();
    let runtime = models::find(&LLAMA_CPP_VULKAN).unwrap();
    let model = models::find(&QWEN3_1_7B_Q4).unwrap();
    let card = rig.entry(&QWEN3_POLISHER);
    assert_eq!(card.model.id, QWEN3_1_7B_Q4);
    assert_eq!(card.requires.len(), 1);
    assert_eq!(&card.requires[0], runtime);
    let total = runtime.transfer_bytes().get() + model.transfer_bytes().get();
    assert_eq!(card.download_bytes.get(), total);
    assert_eq!(card.status, ModelStatus::NotInstalled);
    assert_eq!(
        card.selection,
        EngineSelection::Selectable { active: false }
    );

    assert_eq!(
        block_on(rig.manager.download(&QWEN3_1_7B_Q4)).unwrap(),
        ModelTransferOutcome::Completed
    );
    assert_eq!(rig.stored(&LLAMA_CPP_VULKAN), ModelStatus::Installed);
    assert_eq!(rig.stored(&QWEN3_1_7B_Q4), ModelStatus::Installed);
    assert_eq!(rig.entry(&QWEN3_POLISHER).status, ModelStatus::Installed);
    // One stream under the card's id, never under the runtime's, and the bar only moves forward.
    let progress: Vec<(u64, u64, ModelPhase)> = rig
        .events
        .events()
        .into_iter()
        .filter_map(|event| match event {
            AppEvent::ModelProgress(progress) => {
                assert_eq!(progress.model_id, QWEN3_1_7B_Q4);
                Some((progress.bytes.get(), progress.total.get(), progress.phase))
            }
            _ => None,
        })
        .collect();
    assert!(
        progress
            .iter()
            .all(|(_, card_total, _)| *card_total == total)
    );
    assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert_eq!(progress.last(), Some(&(total, total, ModelPhase::Ready)));
    let phases: Vec<ModelPhase> = progress.iter().map(|(_, _, phase)| *phase).collect();
    let first_check = phases
        .iter()
        .position(|phase| *phase == ModelPhase::Verifying);
    assert!(
        first_check.is_some_and(|at| phases[..at]
            .iter()
            .all(|phase| *phase == ModelPhase::Transferring)),
        "the runtime's own checking never shows half-way: {phases:?}"
    );
}

#[test]
fn a_model_whose_runtime_is_missing_is_partial_and_resuming_fetches_the_runtime() {
    let rig = Rig::new();
    rig.store.install(&QWEN3_1_7B_Q4);
    let model = models::find(&QWEN3_1_7B_Q4).unwrap();
    assert_eq!(
        rig.entry(&QWEN3_POLISHER).status,
        ModelStatus::Partial {
            bytes: model.transfer_bytes()
        }
    );
    assert_eq!(
        app_error(rig.manager.activation(&QWEN3_POLISHER)),
        AppError::ModelMissing {
            model_id: QWEN3_1_7B_Q4
        }
    );
    block_on(rig.manager.download(&QWEN3_1_7B_Q4)).unwrap();
    assert_eq!(rig.stored(&LLAMA_CPP_VULKAN), ModelStatus::Installed);
    assert_eq!(rig.entry(&QWEN3_POLISHER).status, ModelStatus::Installed);
    let writes = rig.manager.activation(&QWEN3_POLISHER).unwrap();
    assert!(writes.contains(&(keys::LLM_ENABLED, SettingValue::Bool(true))));
}

#[test]
fn a_damaged_runtime_damages_the_card_and_a_check_names_it() {
    let rig = Rig::new();
    rig.store.install(&LLAMA_CPP_VULKAN);
    rig.store.install(&QWEN3_1_7B_Q4);
    rig.store.corrupt(&LLAMA_CPP_VULKAN);
    assert_eq!(
        app_error(block_on(rig.manager.verify(&QWEN3_1_7B_Q4))),
        AppError::ModelCorrupt {
            model_id: LLAMA_CPP_VULKAN
        }
    );
    assert_eq!(rig.entry(&QWEN3_POLISHER).status, ModelStatus::Corrupt);
    assert_eq!(
        app_error(rig.manager.activation(&QWEN3_POLISHER)),
        AppError::ModelCorrupt {
            model_id: QWEN3_1_7B_Q4
        }
    );
    rig.store.repair(&LLAMA_CPP_VULKAN);
    block_on(rig.manager.download(&QWEN3_1_7B_Q4)).unwrap();
    assert_eq!(rig.entry(&QWEN3_POLISHER).status, ModelStatus::Installed);
}

#[test]
fn removing_the_llm_stops_its_stage_first_and_takes_its_runtime_along() {
    let rig = Rig::new();
    rig.settings.replace(grammar_polish_on());
    // The chain the session would hold, with the LLM stage built.
    block_on(async {
        rig.polish.for_settings(&rig.settings.current());
    });
    rig.store.install(&LLAMA_CPP_VULKAN);
    rig.store.install(&QWEN3_1_7B_Q4);
    rig.manager.remove(&QWEN3_1_7B_Q4).unwrap();
    assert_eq!(
        rig.llm.unloads(),
        1,
        "the sidecar lets go of its files before they are deleted"
    );
    assert_eq!(rig.stored(&QWEN3_1_7B_Q4), ModelStatus::NotInstalled);
    assert_eq!(rig.stored(&LLAMA_CPP_VULKAN), ModelStatus::NotInstalled);
    assert_eq!(rig.entry(&QWEN3_POLISHER).status, ModelStatus::NotInstalled);
}

#[test]
fn installing_the_selected_llm_warms_the_polish_chain() {
    let rig = Rig::new();
    rig.settings.replace(grammar_polish_on());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        rig.polish.for_settings(&rig.settings.current());
        rig.manager.download(&QWEN3_1_7B_Q4).await.unwrap();
        // The warm-up runs on a task; let it run.
        for _ in 0..20 {
            if rig.llm.prepares() > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
    assert!(rig.llm.prepares() >= 1);
}

#[test]
fn switching_grammar_polish_on_downloads_its_model_unless_offline() {
    let rig = Rig::new();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let offline = settings::resolve([
        (keys::LLM_ENABLED, SettingValue::Bool(true)),
        (keys::OFFLINE_MODE, SettingValue::Bool(true)),
    ]);
    runtime.block_on(async {
        rig.manager
            .fetch_newly_selected(&settings::defaults(), &offline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    });
    assert!(
        rig.phases(&QWEN3_1_7B_Q4).is_empty(),
        "offline mode downloads nothing"
    );

    runtime.block_on(async {
        rig.manager
            .fetch_newly_selected(&settings::defaults(), &grammar_polish_on());
        for _ in 0..100 {
            if rig.stored(&QWEN3_1_7B_Q4).is_installed() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
    assert_eq!(rig.stored(&LLAMA_CPP_VULKAN), ModelStatus::Installed);
    assert_eq!(rig.stored(&QWEN3_1_7B_Q4), ModelStatus::Installed);
    assert_eq!(rig.phases(&QWEN3_1_7B_Q4).last(), Some(&ModelPhase::Ready));

    // Already on: another write (or the same state again) starts nothing.
    let before = rig.events.events().len();
    runtime.block_on(async {
        rig.manager
            .fetch_newly_selected(&grammar_polish_on(), &grammar_polish_on());
        tokio::time::sleep(Duration::from_millis(20)).await;
    });
    assert_eq!(rig.events.events().len(), before);
}
