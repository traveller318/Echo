/*!
 * SOURCE OF TRUTH KEYWORDS: ModelManager, model manager orchestration, download retries, resume after network drop, import from folder, verify model, remove model, activation, ModelsView, damaged model
 * WHAT:  ModelManager: everything the `models_*` commands do. `list` builds the Models page view; `download`
 *        (with automatic resume after a dropped connection), `import` (a folder the user picks) and `verify` run
 *        one transfer per model through the ModelStore with throttled ModelProgress and a terminal phase; `cancel`
 *        stops one; `remove` deletes a model; `activation` checks an engine can be used and returns the setting
 *        writes that select it. After an install or removal the selected speech engine is loaded again.
 * WHY:   02 §8.2 orchestration lives in the pipeline so the adapter only moves and hashes bytes. A dropped
 *        connection (`Network`) is retried after a growing wait, resuming from what arrived (the "Wi-Fi off and on"
 *        case needs no click); the wait resets whenever an attempt moved bytes, so a long download survives many
 *        short drops, and a `waiting` phase tells the page why nothing moves. Offline mode, a hash mismatch or a
 *        full disk are never retried. A model whose hash check failed is remembered as damaged (`Corrupt` in the
 *        list, `ModelCorrupt` for activation) until it is downloaded, imported or removed, because its sizes still
 *        look right. Installing the selected engine's model loads it (so the pill's "Set up" path ends in a ready
 *        engine without a restart), removing it unloads it and asks for a load that answers `ModelMissing`, so the
 *        next press shows "Model not installed" instead of failing a take. Selection and activation come from the
 *        registry entry (`activation` / `selection`), never from an engine name.
 * WHERE: Built by app/bootstrap into CommandCtx; called by ipc/commands/models.rs; watch.rs calls
 *        `check_after_failed_load`.
 */

use std::{collections::HashSet, sync::Arc, time::Duration};

use parking_lot::Mutex;

use super::transfer::{ProgressRelay, TransferSlots};
use crate::{
    pipeline::{
        asr::{AsrWorker, request_load},
        cancel::until_cancelled,
    },
    ports::{EventSink, FolderPicker, ModelStore, PrivacyConsent},
    registry::{
        self,
        engines::EngineEntry,
        permissions::{self, PermissionCtx},
    },
    types::{
        AppError, AppEvent, AppPaths, AsrReadiness, EngineId, EngineKind, EngineRuntime,
        EngineSelection, ModelEntry, ModelId, ModelManifest, ModelPhase, ModelStatus,
        ModelTransferOutcome, ModelsChanged, ModelsView, Permission, PortError, PortResult,
        ResourceKind, SettingKey, SettingValue, SharedSettings,
    },
};

/// The ports and handles the model manager works through.
pub struct ModelDeps {
    pub store: Arc<dyn ModelStore>,
    /// The folder picker behind "Import from folder".
    pub picker: Arc<dyn FolderPicker>,
    /// The speech engine owner, loaded again when its model is installed or removed.
    pub asr: AsrWorker,
    pub settings: SharedSettings,
    /// For the network permission state the page shows (offline mode).
    pub consent: Arc<dyn PrivacyConsent>,
    pub paths: AppPaths,
    pub events: Arc<dyn EventSink<AppEvent>>,
}

/**
 * SOURCE OF TRUTH KEYWORDS: ModelPolicy, download retry delays, progress interval, 10 Hz
 * WHAT:  How long to wait before each resume after a dropped connection, and the progress event interval.
 * WHY:   DEFAULT waits 2 s, 4 s, 8 s, 15 s, then 30 s six times: about four and a half minutes of trying, long
 *        enough to ride out a Wi-Fi reconnect or a laptop lid, short enough that a real outage ends in a visible
 *        failure with the partial download kept (the card then offers Resume). 100 ms is 02 §4.4's 10 Hz.
 * WHERE: ModelManager::new (DEFAULT in the app, zero waits in tests).
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelPolicy {
    pub retry_delays: &'static [Duration],
    pub progress_interval: Duration,
}

impl ModelPolicy {
    pub const DEFAULT: Self = Self {
        retry_delays: &[
            Duration::from_secs(2),
            Duration::from_secs(4),
            Duration::from_secs(8),
            Duration::from_secs(15),
            Duration::from_secs(30),
            Duration::from_secs(30),
            Duration::from_secs(30),
            Duration::from_secs(30),
            Duration::from_secs(30),
            Duration::from_secs(30),
        ],
        progress_interval: Duration::from_millis(100),
    };
}

/// Which kind of transfer ended, for what its outcome means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferKind {
    Download,
    Import,
    Verify,
}

/// Downloads, imports, checks, removes and activates models; clones share one manager.
#[derive(Clone)]
pub struct ModelManager {
    inner: Arc<Inner>,
}

struct Inner {
    deps: ModelDeps,
    policy: ModelPolicy,
    transfers: TransferSlots,
    /// Models whose hash check failed since they were installed.
    damaged: Mutex<HashSet<ModelId>>,
    /// Models already checked after a failed load since they were installed (one check per install).
    checked: Mutex<HashSet<ModelId>>,
}

impl ModelManager {
    pub fn new(deps: ModelDeps, policy: ModelPolicy) -> Self {
        Self {
            inner: Arc::new(Inner {
                deps,
                policy,
                transfers: TransferSlots::default(),
                damaged: Mutex::default(),
                checked: Mutex::default(),
            }),
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: models_list view, ModelsView build, engine runtime from readiness
     * WHAT:  One entry per registry engine that runs a model (status with damage overlaid, selection, runtime,
     *        running transfer) and whether downloads may run now.
     * WHY:   The page renders from this alone; reading disk sizes is a handful of metadata calls.
     * WHERE: `models_list` (on the blocking pool).
     */
    pub fn list(&self) -> PortResult<ModelsView> {
        let inner = &*self.inner;
        let settings = inner.deps.settings.current();
        let readiness = inner.deps.asr.readiness();
        let entries = registry::engines::with_models()
            .map(|(entry, manifest)| {
                let selection = entry.selection(&settings);
                Ok(ModelEntry {
                    engine: entry.spec(),
                    model: manifest.clone(),
                    status: inner.status(manifest)?,
                    runtime: runtime(entry, selection, &readiness),
                    selection,
                    transfer: inner.transfers.latest(&manifest.id),
                })
            })
            .collect::<PortResult<Vec<_>>>()?;
        let network = permissions::check(
            Permission::Network,
            &PermissionCtx {
                settings: &settings,
                consent: inner.deps.consent.as_ref(),
            },
        )?;
        Ok(ModelsView { entries, network })
    }

    /// Downloads `id` (resuming what an earlier try left), unless it is already installed and undamaged.
    pub async fn download(&self, id: &ModelId) -> PortResult<ModelTransferOutcome> {
        let inner = &*self.inner;
        let manifest = transferable(id)?;
        let resume_from = match inner.status(manifest)? {
            ModelStatus::Installed => return Ok(ModelTransferOutcome::Completed),
            ModelStatus::Partial { bytes } => bytes.get(),
            ModelStatus::NotInstalled | ModelStatus::Corrupt => 0,
        };
        let guard = inner.transfers.begin(&manifest.id)?;
        let relay = inner.relay(manifest);
        relay.start(ModelPhase::Transferring, resume_from);
        let result = until_cancelled(
            guard.cancellation(),
            inner.download_with_retries(manifest, &relay),
        )
        .await;
        drop(guard);
        inner.settle(manifest, &relay, TransferKind::Download, result)
    }

    /// Asks the user for a folder and imports `id` from it; closing the picker is `Cancelled`.
    pub async fn import(&self, id: &ModelId) -> PortResult<ModelTransferOutcome> {
        let inner = &*self.inner;
        let manifest = transferable(id)?;
        let title = format!("Choose the folder with the {} files", manifest.label);
        let Some(folder) = inner.deps.picker.pick_folder(&title).await? else {
            return Ok(ModelTransferOutcome::Cancelled);
        };
        let guard = inner.transfers.begin(&manifest.id)?;
        let relay = inner.relay(manifest);
        relay.start(ModelPhase::Transferring, 0);
        let result = until_cancelled(
            guard.cancellation(),
            inner.deps.store.import(manifest, &folder, &relay),
        )
        .await;
        drop(guard);
        inner.settle(manifest, &relay, TransferKind::Import, result)
    }

    /// Re-hashes `id`; a mismatch marks it damaged and is `ModelCorrupt`.
    pub async fn verify(&self, id: &ModelId) -> PortResult<ModelTransferOutcome> {
        let inner = &*self.inner;
        let manifest = registered(id)?;
        let guard = inner.transfers.begin(&manifest.id)?;
        let relay = inner.relay(manifest);
        relay.start(ModelPhase::Verifying, 0);
        let result = until_cancelled(
            guard.cancellation(),
            inner.deps.store.verify(manifest, &relay),
        )
        .await;
        drop(guard);
        inner.settle(manifest, &relay, TransferKind::Verify, result)
    }

    /// Stops `id`'s running download, import or check; false when none runs.
    pub fn cancel(&self, id: &ModelId) -> bool {
        self.inner.transfers.cancel(id)
    }

    /// Deletes `id` and its partial download; the selected engine that ran it then answers `ModelMissing`.
    pub fn remove(&self, id: &ModelId) -> PortResult<()> {
        let inner = &*self.inner;
        let manifest = transferable(id)?;
        if inner.transfers.is_running(&manifest.id) {
            return Err(PortError::new(AppError::Busy)
                .with_detail(format!("`{id}` is being transferred; cancel it first")));
        }
        inner.deps.store.remove(manifest)?;
        inner.forget(&manifest.id);
        inner.announce_changed();
        inner.reload_selected_engines(manifest, true);
        Ok(())
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: engine activation check, models_set_active, use engine, activation writes
     * WHAT:  The setting writes that select `engine_id`, after checking it is registered, chosen by a setting and
     *        its model is installed and undamaged.
     * WHY:   Selecting an engine whose model is missing would make every take fail; refusing here gives the page
     *        one clear error (ModelMissing / ModelCorrupt) instead. The writes themselves go through the settings
     *        write path, which swaps the engine live (02 §8.1).
     * WHERE: `models_set_active`.
     */
    pub fn activation(&self, engine_id: &EngineId) -> PortResult<Vec<(SettingKey, SettingValue)>> {
        let entry = registry::engines::find(engine_id).ok_or_else(|| {
            PortError::new(AppError::NotFound {
                resource: ResourceKind::Engine,
            })
            .with_detail(format!("no engine is registered as `{engine_id}`"))
        })?;
        let writes = entry.activation();
        if writes.is_empty() {
            return Err(AppError::validation("engine_id", "This engine is always on.").into());
        }
        if let Some(manifest) = entry.manifest() {
            match self.inner.status(manifest)? {
                ModelStatus::Installed => {}
                ModelStatus::Corrupt => {
                    return Err(AppError::ModelCorrupt {
                        model_id: manifest.id.clone(),
                    }
                    .into());
                }
                ModelStatus::NotInstalled | ModelStatus::Partial { .. } => {
                    return Err(AppError::ModelMissing {
                        model_id: manifest.id.clone(),
                    }
                    .into());
                }
            }
        }
        Ok(writes)
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: hash check after failed load, verify after load failure, ModelCorrupt detection
     * WHAT:  After `engine_id` failed to load, re-hashes its model once per install; a mismatch marks it damaged.
     * WHY:   02 §8.2: the cheap size check cannot see flipped bytes, and a load failure is the moment they matter.
     *        Once per install, so repeated presses (each re-requests the load) do not re-hash 670 MB every time;
     *        skipped while a transfer of that model runs (it will settle the model anyway).
     * WHERE: watch.rs (ModelWatch) for every failed load the ASR worker reports.
     */
    pub async fn check_after_failed_load(&self, engine_id: &EngineId) {
        let inner = &*self.inner;
        let Some(manifest) = registry::engines::find(engine_id).and_then(EngineEntry::manifest)
        else {
            return;
        };
        if manifest.bundled || !inner.checked.lock().insert(manifest.id.clone()) {
            return;
        }
        if !matches!(
            inner.deps.store.status(manifest),
            Ok(ModelStatus::Installed)
        ) {
            return;
        }
        let Ok(guard) = inner.transfers.begin(&manifest.id) else {
            return;
        };
        tracing::info!(model = %manifest.id, "checking the model after its engine failed to load");
        let relay = inner.relay(manifest);
        relay.start(ModelPhase::Verifying, 0);
        let result = until_cancelled(
            guard.cancellation(),
            inner.deps.store.verify(manifest, &relay),
        )
        .await;
        drop(guard);
        if let Err(error) = inner.settle(manifest, &relay, TransferKind::Verify, result) {
            tracing::warn!(
                model = %manifest.id,
                code = error.error().code().as_str(),
                detail = error.detail(),
                "the model check after a failed load did not pass"
            );
        }
    }
}

impl Inner {
    /// The store's status with a failed hash check overlaid (sizes can look right on damaged files).
    fn status(&self, manifest: &ModelManifest) -> PortResult<ModelStatus> {
        let status = self.deps.store.status(manifest)?;
        Ok(
            if status.is_installed() && self.damaged.lock().contains(&manifest.id) {
                ModelStatus::Corrupt
            } else {
                status
            },
        )
    }

    fn relay<'a>(&'a self, manifest: &'a ModelManifest) -> ProgressRelay<'a> {
        ProgressRelay::new(
            &self.transfers,
            self.deps.events.as_ref(),
            manifest,
            self.policy.progress_interval,
        )
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: download retry loop, resume after dropped connection, waiting phase
     * WHAT:  Downloads until it succeeds, fails with something other than `Network`, or the waits run out; each
     *        attempt resumes where the last stopped.
     * WHY:   See the file header: a network drop is expected on a laptop and costs no click.
     * WHERE: ModelManager::download.
     */
    async fn download_with_retries(
        &self,
        manifest: &ModelManifest,
        relay: &ProgressRelay<'_>,
    ) -> PortResult<()> {
        let mut delays = self.policy.retry_delays.iter();
        loop {
            let reached = relay.bytes();
            let error = match self.deps.store.download(manifest, relay).await {
                Ok(()) => return Ok(()),
                Err(error) if *error.error() == AppError::Network => error,
                Err(error) => return Err(error),
            };
            if relay.bytes() > reached {
                delays = self.policy.retry_delays.iter();
            }
            let Some(delay) = delays.next() else {
                return Err(error);
            };
            tracing::info!(
                model = %manifest.id,
                detail = error.detail(),
                wait_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                "the download stopped; resuming after a wait"
            );
            relay.announce(ModelPhase::Waiting);
            tokio::time::sleep(*delay).await;
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: settle transfer, terminal ModelProgress, ModelsChanged after transfer, after install hook
     * WHAT:  Ends a transfer: announces its terminal phase (ready, cancelled, failed), updates what is known about
     *        the model, tells the page the list changed and loads the selected engine after an install.
     * WHY:   One ending for every transfer kind, so the page always sees the stream end and the list refresh.
     *        A `ModelCorrupt` from a check marks the install damaged; from a download or import it only means the
     *        new files were discarded (an existing install is untouched).
     * WHERE: download, import, verify and check_after_failed_load.
     */
    fn settle(
        &self,
        manifest: &ModelManifest,
        relay: &ProgressRelay<'_>,
        kind: TransferKind,
        result: Option<PortResult<()>>,
    ) -> PortResult<ModelTransferOutcome> {
        let outcome = match result {
            Some(Ok(())) => {
                relay.announce(ModelPhase::Ready);
                self.damaged.lock().remove(&manifest.id);
                if kind != TransferKind::Verify {
                    self.checked.lock().remove(&manifest.id);
                }
                Ok(ModelTransferOutcome::Completed)
            }
            None => {
                relay.announce(ModelPhase::Cancelled);
                Ok(ModelTransferOutcome::Cancelled)
            }
            Some(Err(error)) => {
                if kind == TransferKind::Verify
                    && matches!(error.error(), AppError::ModelCorrupt { .. })
                {
                    self.damaged.lock().insert(manifest.id.clone());
                }
                relay.announce(ModelPhase::Failed);
                Err(error)
            }
        };
        self.announce_changed();
        if kind != TransferKind::Verify && matches!(outcome, Ok(ModelTransferOutcome::Completed)) {
            self.reload_selected_engines(manifest, false);
        }
        outcome
    }

    /// Forgets what was learned about a model's install (it was removed).
    fn forget(&self, id: &ModelId) {
        self.damaged.lock().remove(id);
        self.checked.lock().remove(id);
    }

    fn announce_changed(&self) {
        self.deps.events.emit(ModelsChanged {}.into());
    }

    /// Loads the selected speech engine again when it runs `manifest` (after `unload` when the model was removed).
    fn reload_selected_engines(&self, manifest: &ModelManifest, unload_first: bool) {
        let settings = self.deps.settings.current();
        let affected = registry::engines::with_models().any(|(entry, model)| {
            model.id == manifest.id
                && entry.kind() == EngineKind::Asr
                && entry.selection(&settings) == (EngineSelection::Selectable { active: true })
        });
        if !affected {
            return;
        }
        if unload_first {
            self.deps.asr.unload();
        }
        if let Err(error) = request_load(&self.deps.asr, &settings, &self.deps.paths) {
            tracing::warn!(
                code = error.error().code().as_str(),
                detail = error.detail(),
                "the speech engine could not be loaded after its model changed"
            );
        }
    }
}

/// The registered manifest with `id`.
fn registered(id: &ModelId) -> PortResult<&'static ModelManifest> {
    registry::models::find(id).ok_or_else(|| {
        PortError::new(AppError::NotFound {
            resource: ResourceKind::Model,
        })
        .with_detail(format!("no model is registered as `{id}`"))
    })
}

/// The registered manifest with `id`, when it can be downloaded, imported or removed (not bundled).
fn transferable(id: &ModelId) -> PortResult<&'static ModelManifest> {
    let manifest = registered(id)?;
    if manifest.bundled {
        return Err(AppError::validation("model_id", "This model ships with Echo.").into());
    }
    Ok(manifest)
}

/// What the engine is doing, for an active speech engine the worker knows about.
fn runtime(
    entry: &EngineEntry,
    selection: EngineSelection,
    readiness: &AsrReadiness,
) -> Option<EngineRuntime> {
    if entry.kind() != EngineKind::Asr
        || selection != (EngineSelection::Selectable { active: true })
    {
        return None;
    }
    match readiness {
        AsrReadiness::Loading { engine_id } if *engine_id == entry.id => {
            Some(EngineRuntime::Loading)
        }
        AsrReadiness::Ready {
            engine_id,
            accelerator,
        } if *engine_id == entry.id => Some(EngineRuntime::Ready {
            accelerator: Some(*accelerator),
        }),
        AsrReadiness::Failed { engine_id, error } if *engine_id == entry.id => {
            Some(EngineRuntime::Failed {
                error: error.clone(),
            })
        }
        _ => None,
    }
}
