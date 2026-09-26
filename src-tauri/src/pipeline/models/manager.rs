/*!
 * SOURCE OF TRUTH KEYWORDS: ModelManager, model manager orchestration, download retries, resume after network drop, import from folder, verify model, remove model, activation, ModelsView, damaged model, install set, model requirements, fetch on enable
 * WHAT:  ModelManager: everything the `models_*` commands do. `list` builds the Models page view; `download`
 *        (with automatic resume after a dropped connection), `import` (a folder the user picks) and `verify` run
 *        one transfer per card through the ModelStore with throttled ModelProgress and a terminal phase; `cancel`
 *        stops one; `remove` deletes a model; `activation` checks an engine can be used and returns the setting
 *        writes that select it; `fetch_newly_selected` starts the download of a model polisher the settings just
 *        switched on. A card's transfer covers its install set: the model and what it requires (the LLM's llama.cpp
 *        runtime), requirements first. After an install or removal the engines that run it are loaded or warmed
 *        again.
 * WHY:   02 §8.2 orchestration lives in the pipeline so the adapter only moves and hashes bytes. A dropped
 *        connection (`Network`) is retried after a growing wait, resuming from what arrived (the "Wi-Fi off and on"
 *        case needs no click); the wait resets whenever an attempt moved bytes, so a long download survives many
 *        short drops, and a `waiting` phase tells the page why nothing moves. Offline mode, a hash mismatch or a
 *        full disk are never retried. A model whose hash check failed is remembered as damaged (`Corrupt` in the
 *        list, `ModelCorrupt` for activation) until it is downloaded, imported or removed, because its sizes still
 *        look right. Installing the selected engine's model loads it (so the pill's "Set up" path ends in a ready
 *        engine without a restart), removing it unloads it and asks for a load that answers `ModelMissing`, so the
 *        next press shows "Model not installed" instead of failing a take. A model polisher holds its files open
 *        through its sidecar, and Windows cannot delete or replace an open file, so its stage is unloaded before
 *        its files are removed or replaced and warmed after a new install. A requirement is removed with the last
 *        installed model that needs it. Turning grammar polish on fetches its model and runtime at once (the user
 *        asked for the feature; the Settings page and the Models card show the download), unless offline mode is
 *        on. Selection and activation come from the registry entry (`activation` / `selection`), never from an
 *        engine name.
 * WHERE: Built by app/bootstrap into CommandCtx; called by ipc/commands/models.rs and pipeline/settings_effects.rs;
 *        watch.rs calls `check_after_failed_load`.
 */

use std::{collections::HashSet, sync::Arc, time::Duration};

use parking_lot::Mutex;

use super::transfer::{PartProgress, ProgressRelay, TransferSlots};
use crate::{
    pipeline::{
        asr::{AsrWorker, request_load},
        cancel::until_cancelled,
        polish::PolishChains,
    },
    ports::{EventSink, FolderPicker, ModelStore, PrivacyConsent},
    registry::{
        self,
        engines::EngineEntry,
        permissions::{self, PermissionCtx},
    },
    types::{
        AppError, AppEvent, AppPaths, AsrReadiness, ByteCount, EngineId, EngineKind, EngineRuntime,
        EngineSelection, ModelEntry, ModelId, ModelManifest, ModelPhase, ModelStatus,
        ModelTransferOutcome, ModelsChanged, ModelsView, Permission, PermissionState, PortError,
        PortResult, ResourceKind, SettingKey, SettingValue, SettingsSnapshot, SharedSettings,
    },
};

/// The ports and handles the model manager works through.
pub struct ModelDeps {
    pub store: Arc<dyn ModelStore>,
    /// The folder picker behind "Import from folder".
    pub picker: Arc<dyn FolderPicker>,
    /// The speech engine owner, loaded again when its model is installed or removed.
    pub asr: AsrWorker,
    /// The polish chain, whose model stage is unloaded before its files change and warmed after an install.
    pub polish: PolishChains,
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
     * SOURCE OF TRUTH KEYWORDS: models_list view, ModelsView build, engine runtime from readiness, combined status
     * WHAT:  One entry per registry engine that runs a model (its install set's combined status with damage
     *        overlaid, what it requires, its download size, selection, runtime, running transfer) and whether
     *        downloads may run now.
     * WHY:   The page renders from this alone; reading disk sizes is a handful of metadata calls.
     * WHERE: `models_list` (on the blocking pool).
     */
    pub fn list(&self) -> PortResult<ModelsView> {
        let inner = &*self.inner;
        let settings = inner.deps.settings.current();
        let readiness = inner.deps.asr.readiness();
        let entries = registry::engines::with_models()
            .map(|(entry, manifest)| {
                let set = registry::models::install_set(manifest)?;
                let selection = entry.selection(&settings);
                Ok(ModelEntry {
                    engine: entry.spec(),
                    model: manifest.clone(),
                    requires: requirements(&set)
                        .map(|required| (*required).clone())
                        .collect(),
                    download_bytes: transfer_total(&set),
                    status: inner.set_status(&set)?,
                    runtime: runtime(entry, selection, &readiness),
                    selection,
                    transfer: inner.transfers.latest(&manifest.id),
                })
            })
            .collect::<PortResult<Vec<_>>>()?;
        Ok(ModelsView {
            entries,
            network: inner.network(&settings)?,
        })
    }

    /// Downloads `id` and what it requires (resuming what an earlier try left), unless all are installed and
    /// undamaged.
    pub async fn download(&self, id: &ModelId) -> PortResult<ModelTransferOutcome> {
        let inner = &*self.inner;
        let manifest = transferable(id)?;
        let set = registry::models::install_set(manifest)?;
        let resume_from = match inner.set_status(&set)? {
            ModelStatus::Installed => return Ok(ModelTransferOutcome::Completed),
            ModelStatus::Partial { bytes } => bytes.get(),
            ModelStatus::NotInstalled | ModelStatus::Corrupt => 0,
        };
        let guard = inner
            .transfers
            .begin_set(&manifest.id, &requirement_ids(&set))?;
        let relay = inner.relay(manifest, transfer_total(&set));
        relay.start(ModelPhase::Transferring, resume_from);
        inner.release_replaced(&set)?;
        let result = until_cancelled(guard.cancellation(), inner.download_set(&set, &relay)).await;
        drop(guard);
        inner.settle(manifest, &set, &relay, TransferKind::Download, result)
    }

    /// Asks the user for a folder and imports `id` and what it requires from it; closing the picker is `Cancelled`.
    pub async fn import(&self, id: &ModelId) -> PortResult<ModelTransferOutcome> {
        let inner = &*self.inner;
        let manifest = transferable(id)?;
        let set = registry::models::install_set(manifest)?;
        let title = format!("Choose the folder with the {} files", manifest.label);
        let Some(folder) = inner.deps.picker.pick_folder(&title).await? else {
            return Ok(ModelTransferOutcome::Cancelled);
        };
        let guard = inner
            .transfers
            .begin_set(&manifest.id, &requirement_ids(&set))?;
        let relay = inner.relay(manifest, transfer_total(&set));
        relay.start(ModelPhase::Transferring, 0);
        inner.release_replaced(&set)?;
        let result = until_cancelled(guard.cancellation(), async {
            let mut offset = 0_u64;
            for (index, member) in set.iter().enumerate() {
                let budget = member.transfer_bytes().get();
                // A requirement already installed is kept; the model itself is always imported.
                if index + 1 == set.len() || inner.status(member)? != ModelStatus::Installed {
                    let part = part(&relay, offset, budget, index + 1 == set.len());
                    inner.deps.store.import(member, &folder, &part).await?;
                }
                offset += budget;
            }
            Ok(())
        })
        .await;
        drop(guard);
        inner.settle(manifest, &set, &relay, TransferKind::Import, result)
    }

    /// Re-hashes `id` and what it requires; a mismatch marks that manifest damaged and is `ModelCorrupt`.
    pub async fn verify(&self, id: &ModelId) -> PortResult<ModelTransferOutcome> {
        let inner = &*self.inner;
        let manifest = registered(id)?;
        let set = registry::models::install_set(manifest)?;
        let guard = inner
            .transfers
            .begin_set(&manifest.id, &requirement_ids(&set))?;
        let total = set.iter().map(|member| member.total_bytes().get()).sum();
        let relay = inner.relay(manifest, ByteCount::new(total));
        relay.start(ModelPhase::Verifying, 0);
        let result = until_cancelled(guard.cancellation(), inner.verify_set(&set, &relay)).await;
        drop(guard);
        inner.settle(manifest, &set, &relay, TransferKind::Verify, result)
    }

    /// Stops `id`'s running download, import or check; false when none runs.
    pub fn cancel(&self, id: &ModelId) -> bool {
        self.inner.transfers.cancel(id)
    }

    /// Deletes `id`, its partial download and every requirement no other installed model needs; the selected
    /// engine that ran it then answers `ModelMissing`.
    pub fn remove(&self, id: &ModelId) -> PortResult<()> {
        let inner = &*self.inner;
        let manifest = transferable(id)?;
        let set = registry::models::install_set(manifest)?;
        if set
            .iter()
            .any(|member| inner.transfers.is_running(&member.id))
        {
            return Err(PortError::new(AppError::Busy)
                .with_detail(format!("`{id}` is being transferred; cancel it first")));
        }
        inner.release_stages(&set);
        inner.deps.store.remove(manifest)?;
        inner.forget(&manifest.id);
        for required in requirements(&set) {
            if !inner.needed_by_another(required, manifest)? {
                inner.deps.store.remove(required)?;
                inner.forget(&required.id);
            }
        }
        inner.announce_changed();
        inner.reload_engines(&set, true);
        Ok(())
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: engine activation check, models_set_active, use engine, activation writes
     * WHAT:  The setting writes that select `engine_id`, after checking it is registered, chosen by a setting and
     *        its model and requirements are installed and undamaged.
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
            let set = registry::models::install_set(manifest)?;
            match self.inner.set_status(&set)? {
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
     * SOURCE OF TRUTH KEYWORDS: fetch_newly_selected, download on enable, grammar polish download, auto download LLM
     * WHAT:  When a settings write newly selects a model polisher (grammar polish switched on, or another polish
     *        model chosen while it is on) whose model or runtime is not installed, starts downloading them in the
     *        background; returns at once.
     * WHY:   02 §2.5: the LLM and its runtime are downloaded when the user enables grammar polish, with no second
     *        click. Offline mode skips it (the Settings notice and the Models card say what is missing), a
     *        transfer already running is left alone, and the outcome is only logged: progress and failures reach
     *        the page as ModelProgress like any download. Speech engines are not fetched here: a missing speech model
     *        is set up from the pill or the Models page (02 §8.2).
     * WHERE: pipeline/settings_effects.rs after every settings write.
     */
    pub fn fetch_newly_selected(&self, before: &SettingsSnapshot, after: &SettingsSnapshot) {
        let inner = &*self.inner;
        for (entry, manifest) in registry::engines::with_models() {
            let active = EngineSelection::Selectable { active: true };
            if !entry.is_model_polisher()
                || entry.selection(after) != active
                || entry.selection(before) == active
            {
                continue;
            }
            match inner.network(after) {
                Ok(PermissionState::Granted) => {}
                Ok(_) => {
                    tracing::info!(model = %manifest.id, "offline mode is on; the polish model is not downloaded");
                    continue;
                }
                Err(error) => {
                    tracing::warn!(
                        detail = error.detail(),
                        "the network permission could not be read"
                    );
                    continue;
                }
            }
            let installed = registry::models::install_set(manifest)
                .and_then(|set| inner.set_status(&set))
                .is_ok_and(ModelStatus::is_installed);
            if installed || inner.transfers.is_running(&manifest.id) {
                continue;
            }
            let Ok(runtime) = tokio::runtime::Handle::try_current() else {
                tracing::warn!(model = %manifest.id, "no async runtime; the polish model is not downloaded");
                continue;
            };
            let manager = self.clone();
            let id = manifest.id.clone();
            tracing::info!(model = %id, "grammar polish was switched on; downloading its model");
            runtime.spawn(async move {
                match manager.download(&id).await {
                    Ok(outcome) => {
                        tracing::info!(model = %id, ?outcome, "the polish model download ended");
                    }
                    Err(error) => tracing::warn!(
                        model = %id,
                        code = error.error().code().as_str(),
                        detail = error.detail(),
                        "the polish model could not be downloaded"
                    ),
                }
            });
        }
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
        let set = vec![manifest];
        let relay = inner.relay(manifest, manifest.total_bytes());
        relay.start(ModelPhase::Verifying, 0);
        let result = until_cancelled(guard.cancellation(), inner.verify_set(&set, &relay)).await;
        drop(guard);
        if let Err(error) = inner.settle(manifest, &set, &relay, TransferKind::Verify, result) {
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

    /**
     * SOURCE OF TRUTH KEYWORDS: set_status, combined install status, missing requirement partial
     * WHAT:  One status for a card's install set: Installed when every manifest is; Corrupt when any is damaged;
     *        Partial (bytes already on disk, in transfer units) when anything is there; NotInstalled otherwise.
     * WHY:   The card offers one action for the set: an installed model whose runtime is missing is "Paused" and
     *        its Resume fetches only the runtime; a damaged runtime makes the card "Damaged" so Download again
     *        replaces only what is damaged. A one-manifest set keeps exactly its own status.
     * WHERE: list, download, activation, fetch_newly_selected.
     */
    fn set_status(&self, set: &[&ModelManifest]) -> PortResult<ModelStatus> {
        let mut installed = 0_usize;
        let mut corrupt = false;
        let mut present = false;
        let mut kept = 0_u64;
        for member in set {
            match self.status(member)? {
                ModelStatus::Installed => {
                    installed += 1;
                    present = true;
                    kept = kept.saturating_add(member.transfer_bytes().get());
                }
                ModelStatus::Partial { bytes } => {
                    present = true;
                    kept = kept.saturating_add(bytes.get());
                }
                ModelStatus::Corrupt => corrupt = true,
                ModelStatus::NotInstalled => {}
            }
        }
        Ok(if installed == set.len() {
            ModelStatus::Installed
        } else if corrupt {
            ModelStatus::Corrupt
        } else if present {
            ModelStatus::Partial {
                bytes: ByteCount::new(kept),
            }
        } else {
            ModelStatus::NotInstalled
        })
    }

    /// Whether downloads may run under `settings` (offline mode denies them).
    fn network(&self, settings: &SettingsSnapshot) -> PortResult<PermissionState> {
        permissions::check(
            Permission::Network,
            &PermissionCtx {
                settings,
                consent: self.deps.consent.as_ref(),
            },
        )
    }

    fn relay<'a>(&'a self, manifest: &ModelManifest, total: ByteCount) -> ProgressRelay<'a> {
        ProgressRelay::new(
            &self.transfers,
            self.deps.events.as_ref(),
            manifest.id.clone(),
            total,
            self.policy.progress_interval,
        )
    }

    /// Downloads every manifest of `set` that is not installed and undamaged, in order.
    async fn download_set(
        &self,
        set: &[&'static ModelManifest],
        relay: &ProgressRelay<'_>,
    ) -> PortResult<()> {
        let mut offset = 0_u64;
        for (index, member) in set.iter().enumerate() {
            let budget = member.transfer_bytes().get();
            if self.status(member)? != ModelStatus::Installed {
                let part = part(relay, offset, budget, index + 1 == set.len());
                self.download_with_retries(member, &part, relay).await?;
            }
            offset += budget;
        }
        Ok(())
    }

    /// Re-hashes every manifest of `set`, in order, stopping at the first mismatch.
    async fn verify_set(
        &self,
        set: &[&'static ModelManifest],
        relay: &ProgressRelay<'_>,
    ) -> PortResult<()> {
        let mut offset = 0_u64;
        for (index, member) in set.iter().enumerate() {
            let budget = member.total_bytes().get();
            let part = part(relay, offset, budget, index + 1 == set.len());
            self.deps.store.verify(member, &part).await?;
            offset += budget;
        }
        Ok(())
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: download retry loop, resume after dropped connection, waiting phase
     * WHAT:  Downloads one manifest until it succeeds, fails with something other than `Network`, or the waits run
     *        out; each attempt resumes where the last stopped.
     * WHY:   See the file header: a network drop is expected on a laptop and costs no click.
     * WHERE: download_set.
     */
    async fn download_with_retries(
        &self,
        manifest: &ModelManifest,
        part: &PartProgress<'_, '_>,
        relay: &ProgressRelay<'_>,
    ) -> PortResult<()> {
        let mut delays = self.policy.retry_delays.iter();
        loop {
            let reached = relay.bytes();
            let error = match self.deps.store.download(manifest, part).await {
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
     *        the set's manifests, tells the page the list changed and loads or warms the engines that run it after an
     *        install.
     * WHY:   One ending for every transfer kind, so the page always sees the stream end and the list refresh.
     *        A `ModelCorrupt` from a check marks the manifest it names damaged; from a download or import it only
     *        means the new files were discarded (an existing install is untouched).
     * WHERE: download, import, verify and check_after_failed_load.
     */
    fn settle(
        &self,
        manifest: &ModelManifest,
        set: &[&'static ModelManifest],
        relay: &ProgressRelay<'_>,
        kind: TransferKind,
        result: Option<PortResult<()>>,
    ) -> PortResult<ModelTransferOutcome> {
        let outcome = match result {
            Some(Ok(())) => {
                relay.announce(ModelPhase::Ready);
                for member in set {
                    self.damaged.lock().remove(&member.id);
                    if kind != TransferKind::Verify {
                        self.checked.lock().remove(&member.id);
                    }
                }
                Ok(ModelTransferOutcome::Completed)
            }
            None => {
                relay.announce(ModelPhase::Cancelled);
                Ok(ModelTransferOutcome::Cancelled)
            }
            Some(Err(error)) => {
                if kind == TransferKind::Verify
                    && let AppError::ModelCorrupt { model_id } = error.error()
                {
                    self.damaged.lock().insert(model_id.clone());
                }
                relay.announce(ModelPhase::Failed);
                Err(error)
            }
        };
        tracing::debug!(model = %manifest.id, ?kind, ok = outcome.is_ok(), "model transfer settled");
        self.announce_changed();
        if kind != TransferKind::Verify && matches!(outcome, Ok(ModelTransferOutcome::Completed)) {
            self.reload_engines(set, false);
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

    /// Whether a registered model other than `except` needs `required` and has anything on disk.
    fn needed_by_another(
        &self,
        required: &ModelManifest,
        except: &ModelManifest,
    ) -> PortResult<bool> {
        for other in registry::models::required_by(&required.id) {
            if other.id != except.id && self.deps.store.status(other)? != ModelStatus::NotInstalled
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: release model files, unload polisher before remove, open file on Windows
     * WHAT:  Asks every model polisher whose install set shares a manifest with `set` to release its files (stop
     *        its sidecar).
     * WHY:   Windows refuses to delete or rename a file another process has open, and llama-server maps its model;
     *        the stage starts again on its next use, once the files are back.
     * WHERE: remove; download and import through `release_replaced` when an install is about to be replaced.
     */
    fn release_stages(&self, set: &[&ModelManifest]) {
        for (entry, model) in registry::engines::with_models() {
            if entry.is_model_polisher() && shares_manifest(model, set) {
                self.deps.polish.unload_stage(&entry.id);
            }
        }
    }

    /// Releases the stages using `set` when a transfer is about to replace a damaged install on disk, so its
    /// rename can succeed.
    fn release_replaced(&self, set: &[&ModelManifest]) -> PortResult<()> {
        for member in set {
            if matches!(self.status(member)?, ModelStatus::Corrupt) {
                self.release_stages(set);
                break;
            }
        }
        Ok(())
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: reload engines after install, warm polisher after install, unload after remove
     * WHAT:  After `set` was installed (or removed, `unload_first`), loads the selected speech engine again when it
     *        runs a manifest of the set, and warms the polish chain when the selected model polisher runs one.
     * WHY:   The pill's "Set up" and the Settings toggle must end in a working engine without a restart; after a
     *        removal the speech engine is unloaded so the next press says "Model not installed" (the polisher
     *        released its files before the removal and simply fails over to the rule output until reinstalled).
     * WHERE: settle (after an install) and remove.
     */
    fn reload_engines(&self, set: &[&ModelManifest], unload_first: bool) {
        let settings = self.deps.settings.current();
        let active = EngineSelection::Selectable { active: true };
        let mut speech = false;
        let mut polisher = false;
        for (entry, model) in registry::engines::with_models() {
            if entry.selection(&settings) != active || !shares_manifest(model, set) {
                continue;
            }
            match entry.kind() {
                EngineKind::Asr => speech = true,
                EngineKind::Polisher => polisher = true,
                EngineKind::Vad => {}
            }
        }
        if polisher && !unload_first {
            self.deps.polish.prepare_now(&settings);
        }
        if !speech {
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

/// The requirements of an install set (every manifest but the last).
fn requirements<'a>(
    set: &'a [&'static ModelManifest],
) -> impl Iterator<Item = &'static ModelManifest> + 'a {
    set.iter().take(set.len().saturating_sub(1)).copied()
}

fn requirement_ids(set: &[&'static ModelManifest]) -> Vec<ModelId> {
    requirements(set)
        .map(|required| required.id.clone())
        .collect()
}

/// Bytes a download of the whole set moves.
fn transfer_total(set: &[&ModelManifest]) -> ByteCount {
    ByteCount::new(set.iter().fold(0_u64, |total, member| {
        total.saturating_add(member.transfer_bytes().get())
    }))
}

/// Whether `model`'s install set shares a manifest with `set`.
fn shares_manifest(model: &'static ModelManifest, set: &[&ModelManifest]) -> bool {
    registry::models::install_set(model).is_ok_and(|own| {
        own.iter()
            .any(|member| set.iter().any(|other| other.id == member.id))
    })
}

fn part<'r, 'a>(
    relay: &'r ProgressRelay<'a>,
    offset: u64,
    budget: u64,
    last: bool,
) -> PartProgress<'r, 'a> {
    PartProgress {
        relay,
        offset,
        budget,
        last,
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
