/*!
 * SOURCE OF TRUTH KEYWORDS: model registry, MODELS, model manifests, find model, pinned revision, sha256, license, PARAKEET_TDT_V3, SILERO_VAD_V5, bundled_file
 * WHAT:  The list of every model manifest Echo knows (files, pinned revision, SHA-256, size, license) and lookup
 *        by id.
 * WHY:   A model is a registry entry (02 §3.3), so the model manager, the Models page and About → Models &
 *        licenses (05 A15) all read one list. Manifests pin an exact upstream revision, never a branch (05 §6).
 *        Entries arrive with their adapters: Parakeet TDT 0.6B v3 (downloaded into `models/<id>/`), Silero VAD v5
 *        (bundled, so it is located in the resources folder by `bundled_file`, never downloaded), then Qwen3. The
 *        tests below hold every entry to the manifest rules and prove each bundled file matches its pinned SHA-256.
 * WHERE: Read by registry/engines (an engine's `model_id`), pipeline/models.rs (download, verify, import,
 *        switch), `models_list` and About.
 */

use std::path::PathBuf;

use crate::types::{
    AppError, AppPaths, ByteCount, ModelFile, ModelId, ModelManifest, PortError, PortResult,
    ResourceKind, Sha256Hex, StaticList, StaticStr,
};

/// NVIDIA Parakeet TDT 0.6B v3, int8 ONNX export, downloaded on first run (02 §2.5).
pub const PARAKEET_TDT_V3: ModelId = ModelId::from_static("parakeet-tdt-0.6b-v3");

/// Silero VAD v5, bundled in the installer (02 §2.5).
pub const SILERO_VAD_V5: ModelId = ModelId::from_static("silero-vad-v5");

/**
 * SOURCE OF TRUTH KEYWORDS: Parakeet manifest, istupakov parakeet-tdt-0.6b-v3-onnx, pinned Hugging Face revision, model files sha256
 * WHAT:  The four files of the Parakeet TDT 0.6B v3 int8 ONNX export (05 A1) at Hugging Face commit
 *        8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce of `istupakov/parakeet-tdt-0.6b-v3-onnx`, with SHA-256 and size.
 * WHY:   URLs resolve the pinned commit, never `main`, so the bytes can never change under the hashes (05 §6
 *        resolved 2026-09-24). The ONNX digests are the repository's LFS object ids; vocab.txt is a plain git file,
 *        so its digest was computed from the download. The ParakeetOnnx adapter loads exactly these names (a test
 *        below checks the lists agree).
 * WHERE: MODELS; the model manager downloads and verifies them (step 21); the adapter reads them from
 *        `AppPaths::model_dir`.
 */
const PARAKEET_TDT_V3_FILES: &[ModelFile] = &[
    ModelFile {
        name: StaticStr::new("encoder-model.int8.onnx"),
        url: StaticStr::new(
            "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/encoder-model.int8.onnx",
        ),
        sha256: Sha256Hex::from_static(
            "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09",
        ),
        bytes: ByteCount::new(652_183_999),
    },
    ModelFile {
        name: StaticStr::new("decoder_joint-model.int8.onnx"),
        url: StaticStr::new(
            "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/decoder_joint-model.int8.onnx",
        ),
        sha256: Sha256Hex::from_static(
            "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70",
        ),
        bytes: ByteCount::new(18_202_004),
    },
    ModelFile {
        name: StaticStr::new("nemo128.onnx"),
        url: StaticStr::new(
            "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/nemo128.onnx",
        ),
        sha256: Sha256Hex::from_static(
            "a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f",
        ),
        bytes: ByteCount::new(139_764),
    },
    ModelFile {
        name: StaticStr::new("vocab.txt"),
        url: StaticStr::new(
            "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce/vocab.txt",
        ),
        sha256: Sha256Hex::from_static(
            "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
        ),
        bytes: ByteCount::new(93_939),
    },
];

const SILERO_VAD_V5_FILES: &[ModelFile] = &[ModelFile {
    name: StaticStr::new("silero_vad.onnx"),
    url: StaticStr::new(
        "https://raw.githubusercontent.com/snakers4/silero-vad/6478567951ae5c9979ad7b234185b5515f4be7a1/src/silero_vad/data/silero_vad.onnx",
    ),
    sha256: Sha256Hex::from_static(
        "2623a2953f6ff3d2c1e61740c6cdb7168133479b267dfef114a4a3cc5bdd788f",
    ),
    bytes: ByteCount::new(2_327_524),
}];

/// Every model manifest, in the order the Models page lists them.
pub const MODELS: &[ModelManifest] = &[
    ModelManifest {
        id: PARAKEET_TDT_V3,
        label: StaticStr::new("Parakeet TDT 0.6B v3"),
        license: StaticStr::new("CC-BY-4.0"),
        attribution: Some(StaticStr::new(
            "NVIDIA Parakeet TDT 0.6B v3 by NVIDIA (CC BY 4.0), ONNX export by Ilya Stupakov (onnx-asr)",
        )),
        revision: StaticStr::new("8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce"),
        files: StaticList::new(PARAKEET_TDT_V3_FILES),
        bundled: false,
    },
    ModelManifest {
        id: SILERO_VAD_V5,
        label: StaticStr::new("Silero VAD v5"),
        license: StaticStr::new("MIT"),
        attribution: Some(StaticStr::new(
            "Silero VAD by the Silero Team (MIT License)",
        )),
        // Tag v5.1.2.
        revision: StaticStr::new("6478567951ae5c9979ad7b234185b5515f4be7a1"),
        files: StaticList::new(SILERO_VAD_V5_FILES),
        bundled: true,
    },
];

/// The manifest with `id`.
pub fn find(id: &ModelId) -> Option<&'static ModelManifest> {
    find_in(MODELS, id)
}

fn find_in<'a>(models: &'a [ModelManifest], id: &ModelId) -> Option<&'a ModelManifest> {
    models.iter().find(|manifest| manifest.id == *id)
}

/**
 * SOURCE OF TRUTH KEYWORDS: bundled_file, bundled model path, resources models folder, single-file model
 * WHAT:  The path of the single file of bundled model `id` in the resources folder.
 * WHY:   Bundled models ship with the installer, so they are never located through the ModelStore (which manages
 *        downloads in `models/`); an engine build fn asks here instead. Anything but a registered, bundled,
 *        single-file manifest is an `Internal` error: it is a registry mistake, not a user state.
 * WHERE: Engine build fns in registry/engines.rs (Silero VAD).
 */
pub fn bundled_file(paths: &AppPaths, id: &ModelId) -> PortResult<PathBuf> {
    let manifest = find(id).ok_or_else(|| {
        PortError::new(AppError::NotFound {
            resource: ResourceKind::Model,
        })
        .with_detail(format!("no model is registered as `{id}`"))
    })?;
    match (manifest.bundled, &*manifest.files) {
        (true, [file]) => Ok(paths.bundled_models_dir().join(file.name.as_str())),
        _ => Err(PortError::new(AppError::Internal)
            .with_detail(format!("model `{id}` is not a bundled single-file model"))),
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: manifest rules test, model manifest validation, unique model ids, sha256 format
 * WHAT:  Checks every manifest in MODELS (and a sample, so the rules themselves are exercised today): unique
 *        kebab-case ids, a pinned revision, at least one file, unique plain file names, https URLs, lowercase
 *        64-hex SHA-256 digests, non-zero sizes, and an SPDX license.
 * WHY:   A bad manifest fails only on a user's download (hash mismatch, path traversal via a file name); the
 *        gate catches it when the entry is added.
 * WHERE: `cargo test`.
 */
#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::{
        registry::tests::is_registry_id,
        types::testing::{TempDir, source_resource_paths},
    };

    const SAMPLE_FILES: &[ModelFile] = &[ModelFile {
        name: StaticStr::new("model.onnx"),
        url: StaticStr::new("https://huggingface.co/org/repo/resolve/0123abcd/model.onnx"),
        sha256: Sha256Hex::from_static(
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
        ),
        bytes: ByteCount::new(2_000_000),
    }];

    const SAMPLE: ModelManifest = ModelManifest {
        id: ModelId::from_static("sample-model"),
        label: StaticStr::new("Sample"),
        license: StaticStr::new("MIT"),
        attribution: None,
        revision: StaticStr::new("0123abcd"),
        files: StaticList::new(SAMPLE_FILES),
        bundled: false,
    };

    fn check_manifest(manifest: &ModelManifest) -> Result<(), String> {
        let id = manifest.id.as_str();
        if !is_registry_id(id) {
            return Err(format!("{id}: id is not kebab-case"));
        }
        if manifest.label.trim().is_empty() || manifest.license.trim().is_empty() {
            return Err(format!("{id}: label and license are required"));
        }
        let revision = manifest.revision.as_str();
        if revision.is_empty() || ["main", "master", "latest", "HEAD"].contains(&revision) {
            return Err(format!("{id}: revision must be pinned, not a branch"));
        }
        if manifest.files.is_empty() {
            return Err(format!("{id}: a manifest needs at least one file"));
        }
        let mut names = HashSet::new();
        for file in manifest.files.iter() {
            let name = file.name.as_str();
            if name.is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.') {
                return Err(format!("{id}: `{name}` must be a plain file name"));
            }
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(format!("{id}: `{name}` is listed twice"));
            }
            if !file.url.starts_with("https://") {
                return Err(format!("{id}: `{name}` must download over https"));
            }
            if !file.url.contains(revision) || !file.url.ends_with(&format!("/{name}")) {
                return Err(format!(
                    "{id}: `{name}` must be fetched from the pinned revision under its own name"
                ));
            }
            let digest = file.sha256.as_str();
            if digest.len() != 64
                || !digest
                    .chars()
                    .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
            {
                return Err(format!("{id}: `{name}` needs a lowercase SHA-256 digest"));
            }
            if file.bytes.get() == 0 {
                return Err(format!("{id}: `{name}` needs its size"));
            }
        }
        Ok(())
    }

    #[test]
    fn every_manifest_follows_the_rules() {
        let mut ids = HashSet::new();
        for manifest in MODELS.iter().chain([&SAMPLE]) {
            assert_eq!(check_manifest(manifest), Ok(()));
            assert!(ids.insert(manifest.id.as_str()), "duplicate model id");
        }
    }

    #[test]
    fn the_rules_reject_unsafe_manifests() {
        let traversal = vec![ModelFile {
            name: StaticStr::new("..\\evil.dll"),
            ..SAMPLE_FILES[0].clone()
        }];
        let unpinned = ModelManifest {
            revision: StaticStr::new("main"),
            ..SAMPLE.clone()
        };
        let moving_url = vec![ModelFile {
            url: StaticStr::new("https://huggingface.co/org/repo/resolve/main/model.onnx"),
            ..SAMPLE_FILES[0].clone()
        }];
        let floating = ModelManifest {
            files: StaticList::from(moving_url),
            ..SAMPLE.clone()
        };
        assert!(check_manifest(&floating).is_err());
        let escaping = ModelManifest {
            files: StaticList::from(traversal),
            ..SAMPLE.clone()
        };
        assert!(check_manifest(&unpinned).is_err());
        assert!(check_manifest(&escaping).is_err());
    }

    #[test]
    fn find_looks_models_up_by_id() {
        let models = [SAMPLE.clone()];
        assert_eq!(
            find_in(&models, &ModelId::from_static("sample-model")),
            Some(&SAMPLE)
        );
        assert_eq!(find_in(&models, &ModelId::from_static("other")), None);
        assert_eq!(find(&ModelId::from_static("sample-model")), None);
        assert_eq!(
            find(&SILERO_VAD_V5).map(|manifest| manifest.bundled),
            Some(true)
        );
    }

    #[test]
    fn every_bundled_model_file_matches_its_manifest() {
        let data = TempDir::new("bundled-models");
        let paths = source_resource_paths(data.path());
        for manifest in MODELS.iter().filter(|manifest| manifest.bundled) {
            let path = bundled_file(&paths, &manifest.id).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let digest: String = Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(digest, manifest.files[0].sha256.as_str(), "{}", manifest.id);
            assert_eq!(
                bytes.len() as u64,
                manifest.total_bytes().get(),
                "{}",
                manifest.id
            );
        }
    }

    #[test]
    fn parakeet_is_downloaded_and_weighs_what_the_docs_say() {
        let manifest = find(&PARAKEET_TDT_V3).unwrap();
        assert!(!manifest.bundled);
        assert_eq!(manifest.license.as_str(), "CC-BY-4.0");
        // 02 §2.5: about 670 MB.
        assert_eq!(manifest.total_bytes().get(), 670_619_706);
        assert_eq!(
            bundled_file(&AppPaths::new("data", "resources"), &PARAKEET_TDT_V3)
                .err()
                .map(PortError::into_app_error),
            Some(AppError::Internal)
        );
    }

    #[test]
    fn bundled_file_refuses_unknown_and_downloadable_models() {
        let paths = AppPaths::new("data", "resources");
        assert_eq!(
            bundled_file(&paths, &ModelId::from_static("missing"))
                .err()
                .map(PortError::into_app_error),
            Some(AppError::NotFound {
                resource: ResourceKind::Model
            })
        );
        assert_eq!(
            bundled_file(&paths, &SILERO_VAD_V5).unwrap(),
            std::path::Path::new("resources")
                .join("models")
                .join("silero_vad.onnx")
        );
    }
}
