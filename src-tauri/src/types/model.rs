/*!
 * SOURCE OF TRUTH KEYWORDS: ModelManifest, ModelFile, ModelStatus, ModelPhase, Sha256Hex, model download phase, model install progress, verify, import
 * WHAT:  The model-manager shapes: a model's manifest (ModelManifest with its ModelFile list and SHA-256 digests),
 *        whether it is installed (ModelStatus), and the phases a download or import goes through (ModelPhase).
 * WHY:   The manifest is data the registry declares (02 §3.3 `models`) and the ModelStore adapter needs to fetch
 *        and verify files, yet adapters may not import the registry (02 §3.2), so the shape lives here and the
 *        registry hands the entry to the port. `revision` pins the exact upstream commit the hashes were computed
 *        from (05 §6), never a moving branch. `total_bytes` is derived from the files so it cannot drift.
 *        Download and import-from-disk share one progress stream (02 §8.2): bytes move, hashes are checked, then
 *        the `.partial` folder is renamed into place. A terminal phase tells the UI the stream has ended without
 *        polling; the failure detail comes back as the command's AppError. ModelStatus carries no filesystem
 *        path because it crosses IPC; `ModelStore::locate` returns the path inside Rust.
 * WHERE: Manifests in registry/models; `ModelStore` (ports/model_store.rs) takes them; ModelStatus and
 *        ModelManifest reach the Models page through `models_list`; ModelPhase travels in ModelProgress.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{ByteCount, ModelId, StaticList, StaticStr, ids::static_str_id};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelPhase {
    /// Bytes are arriving from the network or being copied from the chosen folder.
    Transferring,
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
}
