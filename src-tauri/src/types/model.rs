/*!
 * SOURCE OF TRUTH KEYWORDS: ModelManifest, ModelFile, ModelStatus, ModelPhase, Sha256Hex, ModelEntry, ModelsView, EngineSelection, EngineRuntime, ModelTransferOutcome
 * WHAT:  The model-manager shapes: a model's manifest (ModelManifest with its ModelFile list and SHA-256 digests),
 *        whether it is installed (ModelStatus), the phases a download or import goes through (ModelPhase), how a
 *        transfer ended (ModelTransferOutcome), the models command inputs (ModelInput, EngineInput) and the Models
 *        page's view (ModelsView: one ModelEntry per engine that runs a model, with its selection and runtime).
 * WHY:   The manifest is data the registry declares (02 §3.3 `models`) and the ModelStore adapter needs to fetch
 *        and verify files, yet adapters may not import the registry (02 §3.2), so the shape lives here and the
 *        registry hands the entry to the port. `revision` pins the exact upstream commit the hashes were computed
 *        from (05 §6), never a moving branch. `total_bytes` is derived from the files so it cannot drift.
 *        Download and import-from-disk share one progress stream (02 §8.2): bytes move, hashes are checked, then
 *        the `.partial` folder is renamed into place. A terminal phase tells the UI the stream has ended without
 *        polling; the failure detail comes back as the command's AppError. ModelStatus carries no filesystem
 *        path because it crosses IPC; `ModelStore::locate` returns the path inside Rust. A ModelEntry is complete
 *        for one card (engine caps, manifest, status, whether settings select it, what it is doing, the transfer
 *        running now), so the page renders without joining lists and keeps no copy of domain state.
 * WHERE: Manifests in registry/models; `ModelStore` (ports/model_store.rs) takes them; pipeline/models builds
 *        ModelsView for `models_list`; ModelPhase travels in ModelProgress; the inputs are declared by
 *        ipc/commands/models.rs.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{
    Accelerator, AppError, ByteCount, EngineId, EngineSpec, ModelId, ModelProgress,
    PermissionState, StaticList, StaticStr, ids::static_str_id,
};

static_str_id! {
    /// A SHA-256 digest as 64 lowercase hex characters.
    Sha256Hex
}

/// One file of a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelFile {
    /// File name inside the model folder, e.g. `encoder-model.int8.onnx`; never a path with separators.
    pub name: StaticStr,
    /// Where it is downloaded from; for a bundled model, the upstream it was taken from.
    pub url: StaticStr,
    pub sha256: Sha256Hex,
    pub bytes: ByteCount,
}

/// Everything needed to fetch, verify and credit a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelManifest {
    pub id: ModelId,
    pub label: StaticStr,
    /// SPDX license id, e.g. `CC-BY-4.0`, listed in About → Models & licenses (05 A15).
    pub license: StaticStr,
    /// Credit line the license requires; None when it requires none.
    pub attribution: Option<StaticStr>,
    /// Upstream revision (commit hash) the digests were computed from.
    pub revision: StaticStr,
    pub files: StaticList<ModelFile>,
    /// Ships inside the installer's resources, so it is never downloaded.
    pub bundled: bool,
}

impl ModelManifest {
    /// Sum of every file's size.
    pub fn total_bytes(&self) -> ByteCount {
        ByteCount::new(
            self.files
                .iter()
                .fold(0_u64, |total, file| total.saturating_add(file.bytes.get())),
        )
    }
}

/// Whether a model is on disk and usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelStatus {
    NotInstalled,
    /// An interrupted download left `bytes` in the `.partial` folder; the next download resumes from there.
    Partial {
        bytes: ByteCount,
    },
    /// Every file is in place with the manifest's size (hashes are checked on demand, 02 §8.2).
    Installed,
    /// The installed files do not match the manifest: a size differs or a file is gone (the startup check), or a
    /// hash check failed since the model was installed. Downloading again replaces it.
    Corrupt,
}

impl ModelStatus {
    /// The model can be used as it is.
    pub const fn is_installed(self) -> bool {
        matches!(self, Self::Installed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelPhase {
    /// Bytes are arriving from the network or being copied from the chosen folder.
    Transferring,
    /// The connection dropped; the download resumes from where it stopped after a short wait.
    Waiting,
    /// SHA-256 of every file is being checked.
    Verifying,
    /// Verified files are being moved into `models/<id>/`.
    Installing,
    /// The model is installed and usable.
    Ready,
    Cancelled,
    Failed,
}

impl ModelPhase {
    /// No further progress follows this phase.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Ready | Self::Cancelled | Self::Failed)
    }
}

/// How a download or import ended when it did not fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelTransferOutcome {
    /// The model is installed and verified.
    Completed,
    /// The user stopped it (or closed the folder picker); a partial download is kept for the next try.
    Cancelled,
}

/**
 * SOURCE OF TRUTH KEYWORDS: ModelInput, EngineInput, models command input, model id input, engine id input
 * WHAT:  The input of the models commands that act on one model (download, cancel, import, verify, remove) and of
 *        `models_set_active` (one engine).
 * WHY:   A struct, not a bare id, so a command can grow options without changing its call sites. The declared schema
 *        (garde) refuses an id that could not be a registry id before any lookup; whether it is registered is the
 *        registry's answer (`NotFound { model | engine }`).
 * WHERE: ipc/commands/models.rs; built in the UI by the Models page actions.
 */
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, garde::Validate)]
pub struct ModelInput {
    #[garde(custom(well_formed_model))]
    pub model_id: ModelId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, garde::Validate)]
pub struct EngineInput {
    #[garde(custom(well_formed_engine))]
    pub engine_id: EngineId,
}

fn well_formed_model(id: &ModelId, (): &()) -> garde::Result {
    if id.is_well_formed() {
        Ok(())
    } else {
        Err(garde::Error::new("Use a model id from the registry."))
    }
}

fn well_formed_engine(id: &EngineId, (): &()) -> garde::Result {
    if id.is_well_formed() {
        Ok(())
    } else {
        Err(garde::Error::new("Use an engine id from the registry."))
    }
}

/// How an engine comes to be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineSelection {
    /// Used on every take whenever it is installed (a bundled detector); there is nothing to choose.
    BuiltIn,
    /// Chosen by a setting (`models_set_active` writes it); `active` when the settings select it now.
    Selectable { active: bool },
}

/// What a selected engine is doing now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineRuntime {
    /// Its model is loading and warming up.
    Loading,
    /// Loaded and warm; `accelerator` when the engine runs on one (speech engines).
    Ready { accelerator: Option<Accelerator> },
    /// It could not be loaded; takes fail with `error` until it is fixed (e.g. `ModelCorrupt`).
    Failed { error: AppError },
}

/// One card of the Models page: an engine that runs a model, and everything the card shows or offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelEntry {
    pub engine: EngineSpec,
    pub model: ModelManifest,
    pub status: ModelStatus,
    pub selection: EngineSelection,
    /// What the engine is doing, when the settings select it and it has been asked to load.
    pub runtime: Option<EngineRuntime>,
    /// The download, import or check running for this model now, with its latest progress.
    pub transfer: Option<ModelProgress>,
}

/// Everything the Models page renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelsView {
    /// One entry per registry engine that runs a model, in registry order.
    pub entries: Vec<ModelEntry>,
    /// Whether downloads may run now (offline mode denies them); importing from a folder works either way.
    pub network: PermissionState,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const FILES: &[ModelFile] = &[
        ModelFile {
            name: StaticStr::new("encoder.onnx"),
            url: StaticStr::new("https://huggingface.co/org/model/resolve/abc123/encoder.onnx"),
            sha256: Sha256Hex::from_static(
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
            bytes: ByteCount::new(600),
        },
        ModelFile {
            name: StaticStr::new("vocab.txt"),
            url: StaticStr::new("https://huggingface.co/org/model/resolve/abc123/vocab.txt"),
            sha256: Sha256Hex::from_static(
                "1111111111111111111111111111111111111111111111111111111111111111",
            ),
            bytes: ByteCount::new(70),
        },
    ];

    const MANIFEST: ModelManifest = ModelManifest {
        id: ModelId::from_static("test-model"),
        label: StaticStr::new("Test model"),
        license: StaticStr::new("MIT"),
        attribution: None,
        revision: StaticStr::new("abc123"),
        files: StaticList::new(FILES),
        bundled: false,
    };

    #[test]
    fn manifests_are_const_and_sum_their_files() {
        assert_eq!(MANIFEST.total_bytes(), ByteCount::new(670));
        let value = serde_json::to_value(&MANIFEST).unwrap();
        assert_eq!(value["files"][1]["name"], json!("vocab.txt"));
        assert_eq!(
            serde_json::from_value::<ModelManifest>(value).unwrap(),
            MANIFEST
        );
    }

    #[test]
    fn status_is_tagged_by_kind() {
        assert_eq!(
            serde_json::to_value(ModelStatus::Partial {
                bytes: ByteCount::new(10)
            })
            .unwrap(),
            json!({ "kind": "partial", "bytes": 10 })
        );
        assert_eq!(
            serde_json::to_value(ModelStatus::Installed).unwrap(),
            json!({ "kind": "installed" })
        );
    }

    #[test]
    fn only_ready_cancelled_and_failed_end_the_stream() {
        let terminal: Vec<ModelPhase> = [
            ModelPhase::Transferring,
            ModelPhase::Waiting,
            ModelPhase::Verifying,
            ModelPhase::Installing,
            ModelPhase::Ready,
            ModelPhase::Cancelled,
            ModelPhase::Failed,
        ]
        .into_iter()
        .filter(|phase| phase.is_terminal())
        .collect();
        assert_eq!(
            terminal,
            [ModelPhase::Ready, ModelPhase::Cancelled, ModelPhase::Failed]
        );
    }

    #[test]
    fn inputs_refuse_ids_that_could_not_be_registered() {
        use garde::Validate;

        let valid = ModelInput {
            model_id: ModelId::from_static("parakeet-tdt-0.6b-v3"),
        };
        assert!(valid.validate().is_ok());
        for junk in ["", "Parakeet", "../models", "a b", "x--y"] {
            let model = ModelInput {
                model_id: ModelId::from(junk.to_owned()),
            };
            assert!(model.validate().is_err(), "{junk}");
            let engine = EngineInput {
                engine_id: EngineId::from(junk.to_owned()),
            };
            assert!(engine.validate().is_err(), "{junk}");
        }
    }

    #[test]
    fn selection_and_runtime_are_tagged_by_kind() {
        assert_eq!(
            serde_json::to_value(EngineSelection::Selectable { active: true }).unwrap(),
            json!({ "kind": "selectable", "active": true })
        );
        assert_eq!(
            serde_json::to_value(EngineRuntime::Ready {
                accelerator: Some(Accelerator::Cpu)
            })
            .unwrap(),
            json!({ "kind": "ready", "accelerator": "cpu" })
        );
        assert_eq!(
            serde_json::to_value(ModelStatus::Corrupt).unwrap(),
            json!({ "kind": "corrupt" })
        );
        assert!(ModelStatus::Installed.is_installed());
        assert!(!ModelStatus::Corrupt.is_installed());
    }
}
