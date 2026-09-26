/*!
 * SOURCE OF TRUTH KEYWORDS: model registry, MODELS, model manifests, find model, pinned revision, sha256, license, PARAKEET_TDT_V3, SILERO_VAD_V5, QWEN3_1_7B_Q4, LLAMA_CPP_VULKAN, bundled_file, install_set
 * WHAT:  The list of every model and runtime manifest Echo knows (files, pinned revision, SHA-256, size, license,
 *        what it requires), lookup by id, and a model's install set (its requirements, then itself).
 * WHY:   A model is a registry entry (02 §3.3), so the model manager, the Models page and About → Models &
 *        licenses (05 A15) all read one list. Manifests pin an exact upstream revision, never a branch (05 §6).
 *        Entries arrive with their adapters: Parakeet TDT 0.6B v3 (downloaded into `models/<id>/`), Silero VAD v5
 *        (bundled, so it is located in the resources folder by `bundled_file`, never downloaded), Qwen3 1.7B for
 *        grammar polish and the llama.cpp runtime it runs on (a release archive unpacked into `runtimes/<id>/`,
 *        which Qwen3 `requires`). The tests below hold every entry to the manifest rules and prove each bundled file
 *        matches its pinned SHA-256.
 * WHERE: Read by registry/engines (an engine's `model_id`, the LLM build fn's file names), pipeline/models
 *        (download, verify, import, switch, install sets), `models_list` and About.
 */

use std::path::PathBuf;

use crate::types::{
    AppError, AppPaths, ByteCount, ModelFile, ModelId, ModelKind, ModelManifest, PortError,
    PortResult, ResourceKind, Sha256Hex, StaticList, StaticStr,
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

/// llama.cpp's `llama-server` (Windows x64, Vulkan build), the runtime the LLM polisher starts (02 §2.5).
pub const LLAMA_CPP_VULKAN: ModelId = ModelId::from_static("llama-cpp-vulkan");

/// Qwen3 1.7B, Q4_K_M GGUF: the grammar polish model (02 §2.5, 05 A16).
pub const QWEN3_1_7B_Q4: ModelId = ModelId::from_static("qwen3-1.7b-q4-k-m");

/// The pinned llama.cpp release archive every runtime file is unpacked from.
const LLAMA_CPP_ARCHIVE_URL: &str = "https://github.com/ggml-org/llama.cpp/releases/download/b11146/llama-b11146-bin-win-vulkan-x64.zip";

/// One file kept from the llama.cpp archive, with its size and digest as unpacked.
const fn runtime_file(name: &'static str, sha256: &'static str, bytes: u64) -> ModelFile {
    ModelFile {
        name: StaticStr::new(name),
        url: StaticStr::new(LLAMA_CPP_ARCHIVE_URL),
        sha256: Sha256Hex::from_static(sha256),
        bytes: ByteCount::new(bytes),
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: llama.cpp runtime manifest, llama-server files, b11146, Vulkan build, release archive, kept files
 * WHAT:  The files Echo keeps from llama.cpp release b11146 (commit 7fe450e19305b828c199d602c23a8337aaa1f03b, the build
 *        of stable release v0.5.0, 2026-09-23), `llama-b11146-bin-win-vulkan-x64.zip`: `llama-server.exe` and every
 *        library it loads (its implementation DLL, llama, mtmd, ggml with the Vulkan backend and all CPU variants,
 *        the OpenMP runtime) plus the OpenMP license, each with its unpacked size and SHA-256.
 * WHY:   The release ships ~55 files (other tools, the RPC backend); only what `llama-server` imports or loads at
 *        start is kept (checked with `dumpbin /dependents`; ggml picks the CPU variant for the processor at runtime,
 *        so all of them stay). The archive digest is GitHub's published asset digest; the member digests were
 *        computed from the downloaded archive. The Visual C++ runtime the binaries import is not in the release: the
 *        adapter puts Echo's bundled copy on the child's PATH (05 A20).
 * WHERE: The LLAMA_CPP_VULKAN entry of MODELS; the LLM polisher's build fn names `llama-server.exe` in it.
 */
const LLAMA_CPP_VULKAN_FILES: &[ModelFile] = &[
    runtime_file(
        "llama-server.exe",
        "7b886298b688509ced3e92b420edd57dd3d665da72c1fc207d7a537be5870352",
        9_216,
    ),
    runtime_file(
        "llama-server-impl.dll",
        "fae1ca8959986062a24c7b0a92e12a782f2770fc5448c7cc631f0d4bb6cac4a0",
        8_916_992,
    ),
    runtime_file(
        "llama-common.dll",
        "5a07bd8dfb7626476a47b4931e278c82518991b4faf85ce42cde82eb0ce6bb50",
        7_824_896,
    ),
    runtime_file(
        "llama.dll",
        "8bc4713fde3b48a35c06c6d503c9fd128d349275e6fad6b48d235796c9d9739f",
        3_175_936,
    ),
    runtime_file(
        "mtmd.dll",
        "592ed18ee70d874c9d80ec0c9eb014258c10d9087462e03f8fc00b9acfb47b9a",
        1_773_568,
    ),
    runtime_file(
        "ggml.dll",
        "da454325a1cdb658c469983ef78bc7c23a2729394607c2583446db4c19f7bcda",
        79_872,
    ),
    runtime_file(
        "ggml-base.dll",
        "be315e18c795d15658f5d13b4a6e0b4cd534b7d69f6d4093db60ea74b0ada159",
        795_648,
    ),
    runtime_file(
        "ggml-vulkan.dll",
        "7a5c5ec81b01678ca2fc8203e9f37b02a80e55299cff06c629e5d39b6d1e4778",
        44_263_936,
    ),
    runtime_file(
        "ggml-cpu-alderlake.dll",
        "ccd9f3c469b360afd285a2a96c84474483e0be728046f4fdd5b1a71ed735648a",
        1_256_448,
    ),
    runtime_file(
        "ggml-cpu-cannonlake.dll",
        "e90b05f511dcc5df41f1a893c2e0aa9d5d20cc1ee41a064b1f85e7ed3c76da5e",
        1_474_560,
    ),
    runtime_file(
        "ggml-cpu-cascadelake.dll",
        "d30b718b83994b5fb582486145cbda054d5431f3699da9e938930132777b2530",
        1_459_712,
    ),
    runtime_file(
        "ggml-cpu-cooperlake.dll",
        "7c31b2439675d8c4be74a6a84d4d7747805ba9c51253ec4d87e7a3f767048877",
        1_460_224,
    ),
    runtime_file(
        "ggml-cpu-haswell.dll",
        "18c5489f5ef7260f198d18ba53056a63bfec625d7f91a9bee1a5226baea167bd",
        1_262_592,
    ),
    runtime_file(
        "ggml-cpu-icelake.dll",
        "55a4dde89c6e27142c7f7157b3eca2cbec15ae2ba6bc14172e4d73bb97bca3de",
        1_465_856,
    ),
    runtime_file(
        "ggml-cpu-ivybridge.dll",
        "bdca4d6649f1776f74d20698dab32f52d7cce4fd3e216facf6d27a461e7d31c6",
        1_153_024,
    ),
    runtime_file(
        "ggml-cpu-piledriver.dll",
        "2daf851d57d4556acafc01cb69626603ce13b7bbbe147ebaa471f67d96b65a8c",
        1_157_120,
    ),
    runtime_file(
        "ggml-cpu-sandybridge.dll",
        "6c6edc12155f798bdb6e82bdf905179f921dc250a8bd81f5a494cdeafa3ceb68",
        1_133_056,
    ),
    runtime_file(
        "ggml-cpu-sapphirerapids.dll",
        "dd05f6c9dfb65141c8532994d0a55014cbf8f1a0400fe83f8ebded6bef3c46e1",
        1_737_216,
    ),
    runtime_file(
        "ggml-cpu-skylakex.dll",
        "8a0b58839ac30b5d72c58808604e33e6f3262a43bd78167e0782f9945357700e",
        1_467_904,
    ),
    runtime_file(
        "ggml-cpu-sse42.dll",
        "9f824721d0df4bd8ec6849b1ff550b4d02dcc5d53a016ec41237ffbbd0417f03",
        958_464,
    ),
    runtime_file(
        "ggml-cpu-x64.dll",
        "ff312c4aa238d96ff6297384469235e37dde22a82df442d5b903546cf997b9c2",
        949_248,
    ),
    runtime_file(
        "ggml-cpu-zen4.dll",
        "06660a9ef42529bd49ee9f0fbc517a966c8b188097157c0eff0bbda256bdf75b",
        1_466_880,
    ),
    runtime_file(
        "libomp.dll",
        "a12116ba72d1d6820407cf30be23da04ce79d6bb8a71a5ee71759c5a1faa6f1c",
        768_000,
    ),
    runtime_file(
        "LICENSE-LLVM-OpenMP",
        "fdad1758a9e1f9d5a81e18879b3406772115edc92c24bfa36b70c654f325e8e4",
        19_741,
    ),
];

/// The `llama-server` executable inside the runtime folder (a file of LLAMA_CPP_VULKAN_FILES).
pub const LLAMA_SERVER_EXE: &str = "llama-server.exe";

const LLAMA_CPP_VULKAN_ARCHIVE: ModelFile = ModelFile {
    name: StaticStr::new("llama-b11146-bin-win-vulkan-x64.zip"),
    url: StaticStr::new(LLAMA_CPP_ARCHIVE_URL),
    sha256: Sha256Hex::from_static(
        "55a378aa095b466979d85075234f66d7655c7a7483222af0c006c0e55b4d7bd6",
    ),
    bytes: ByteCount::new(32_127_004),
};

/**
 * SOURCE OF TRUTH KEYWORDS: Qwen3 manifest, Qwen3-1.7B-Q4_K_M.gguf, ggml-org GGUF, pinned revision, grammar model file
 * WHAT:  The one GGUF file of Qwen3 1.7B at Q4_K_M from `ggml-org/Qwen3-1.7B-GGUF` at commit
 *        daeb8e2d528a760970442092f6bf1e55c3b659eb, with its SHA-256 (the repository's LFS object id) and size.
 * WHY:   ggml-org publishes the GGUF conversions the llama.cpp project itself tests; Qwen's own repository has no
 *        Q4_K_M for 1.7B. Q4_K_M keeps grammar quality at about 1.3 GB (05 §6 resolved 2026-09-26).
 * WHERE: QWEN3_1_7B_Q4 (MODELS); the LLM polisher's build fn names the file.
 */
const QWEN3_1_7B_Q4_FILES: &[ModelFile] = &[ModelFile {
    name: StaticStr::new("Qwen3-1.7B-Q4_K_M.gguf"),
    url: StaticStr::new(
        "https://huggingface.co/ggml-org/Qwen3-1.7B-GGUF/resolve/daeb8e2d528a760970442092f6bf1e55c3b659eb/Qwen3-1.7B-Q4_K_M.gguf",
    ),
    sha256: Sha256Hex::from_static(
        "d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5",
    ),
    bytes: ByteCount::new(1_282_439_264),
}];

/// What the Qwen3 model needs installed to run: the llama.cpp runtime.
const QWEN3_1_7B_Q4_REQUIRES: &[ModelId] = &[LLAMA_CPP_VULKAN];

/// Every model manifest, in the order the Models page lists them.
pub const MODELS: &[ModelManifest] = &[
    ModelManifest {
        id: PARAKEET_TDT_V3,
        label: StaticStr::new("Parakeet TDT 0.6B v3"),
        kind: ModelKind::Model,
        license: StaticStr::new("CC-BY-4.0"),
        attribution: Some(StaticStr::new(
            "NVIDIA Parakeet TDT 0.6B v3 by NVIDIA (CC BY 4.0), ONNX export by Ilya Stupakov (onnx-asr)",
        )),
        revision: StaticStr::new("8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce"),
        files: StaticList::new(PARAKEET_TDT_V3_FILES),
        archive: None,
        requires: StaticList::new(&[]),
        bundled: false,
    },
    ModelManifest {
        id: SILERO_VAD_V5,
        label: StaticStr::new("Silero VAD v5"),
        kind: ModelKind::Model,
        license: StaticStr::new("MIT"),
        attribution: Some(StaticStr::new(
            "Silero VAD by the Silero Team (MIT License)",
        )),
        // Tag v5.1.2.
        revision: StaticStr::new("6478567951ae5c9979ad7b234185b5515f4be7a1"),
        files: StaticList::new(SILERO_VAD_V5_FILES),
        archive: None,
        requires: StaticList::new(&[]),
        bundled: true,
    },
    ModelManifest {
        id: QWEN3_1_7B_Q4,
        label: StaticStr::new("Qwen3 1.7B (Q4_K_M)"),
        kind: ModelKind::Model,
        license: StaticStr::new("Apache-2.0"),
        attribution: Some(StaticStr::new(
            "Qwen3-1.7B by the Qwen team, Alibaba Cloud (Apache 2.0), GGUF conversion by ggml-org",
        )),
        revision: StaticStr::new("daeb8e2d528a760970442092f6bf1e55c3b659eb"),
        files: StaticList::new(QWEN3_1_7B_Q4_FILES),
        archive: None,
        requires: StaticList::new(QWEN3_1_7B_Q4_REQUIRES),
        bundled: false,
    },
    ModelManifest {
        id: LLAMA_CPP_VULKAN,
        label: StaticStr::new("llama.cpp runtime b11146 (Vulkan)"),
        kind: ModelKind::Runtime,
        license: StaticStr::new("MIT"),
        attribution: Some(StaticStr::new(
            "llama.cpp by the ggml authors (MIT License), with the LLVM OpenMP runtime (Apache-2.0 WITH LLVM-exception)",
        )),
        revision: StaticStr::new("b11146"),
        files: StaticList::new(LLAMA_CPP_VULKAN_FILES),
        archive: Some(LLAMA_CPP_VULKAN_ARCHIVE),
        requires: StaticList::new(&[]),
        bundled: false,
    },
];

/// The manifest with `id`.
pub fn find(id: &ModelId) -> Option<&'static ModelManifest> {
    find_in(MODELS, id)
}

/**
 * SOURCE OF TRUTH KEYWORDS: install_set, model requirements, requires runtime, install order, requirements first
 * WHAT:  `manifest` and every manifest it requires, requirements first and the model last, each once.
 * WHY:   The model manager downloads, imports, checks and counts a model together with what it runs on (an LLM and
 *        its llama.cpp runtime), and installs a runtime before the model so a finished model is usable at once.
 *        A requirement that is not registered is `NotFound { model }`: a registry mistake the tests below rule out.
 * WHERE: pipeline/models (every transfer and the Models page view).
 */
pub fn install_set(manifest: &'static ModelManifest) -> PortResult<Vec<&'static ModelManifest>> {
    install_set_in(MODELS, manifest)
}

fn install_set_in<'a>(
    models: &'a [ModelManifest],
    manifest: &'a ModelManifest,
) -> PortResult<Vec<&'a ModelManifest>> {
    let mut set: Vec<&ModelManifest> = Vec::with_capacity(manifest.requires.len() + 1);
    for id in manifest.requires.iter() {
        let required = find_in(models, id).ok_or_else(|| {
            PortError::new(AppError::NotFound {
                resource: ResourceKind::Model,
            })
            .with_detail(format!(
                "`{}` requires `{id}`, which is not registered",
                manifest.id
            ))
        })?;
        if !set.iter().any(|known| known.id == required.id) {
            set.push(required);
        }
    }
    set.push(manifest);
    Ok(set)
}

/// Every registered manifest that lists `id` as a requirement.
pub fn required_by(id: &ModelId) -> impl Iterator<Item = &'static ModelManifest> + '_ {
    MODELS
        .iter()
        .filter(move |manifest| manifest.requires.contains(id))
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
        kind: ModelKind::Model,
        license: StaticStr::new("MIT"),
        attribution: None,
        revision: StaticStr::new("0123abcd"),
        files: StaticList::new(SAMPLE_FILES),
        archive: None,
        requires: StaticList::new(&[]),
        bundled: false,
    };

    const SAMPLE_ARCHIVE_URL: &str = "https://github.com/org/tool/releases/download/v1/tool-v1.zip";

    const SAMPLE_RUNTIME_FILES: &[ModelFile] = &[ModelFile {
        name: StaticStr::new("tool.exe"),
        url: StaticStr::new(SAMPLE_ARCHIVE_URL),
        sha256: Sha256Hex::from_static(
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
        ),
        bytes: ByteCount::new(4_000),
    }];

    const SAMPLE_RUNTIME: ModelManifest = ModelManifest {
        id: ModelId::from_static("sample-runtime"),
        label: StaticStr::new("Sample runtime"),
        kind: ModelKind::Runtime,
        license: StaticStr::new("MIT"),
        attribution: None,
        revision: StaticStr::new("v1"),
        files: StaticList::new(SAMPLE_RUNTIME_FILES),
        archive: Some(ModelFile {
            name: StaticStr::new("tool-v1.zip"),
            url: StaticStr::new(SAMPLE_ARCHIVE_URL),
            sha256: Sha256Hex::from_static(
                "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            ),
            bytes: ByteCount::new(1_500),
        }),
        requires: StaticList::new(&[]),
        bundled: false,
    };

    /// Checks one downloadable file: plain unique name, https, pinned URL, digest and size.
    fn check_file(
        id: &str,
        file: &ModelFile,
        revision: &str,
        names: &mut HashSet<String>,
    ) -> Result<(), String> {
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
        if !file.url.contains(revision) {
            return Err(format!("{id}: `{name}` must come from the pinned revision"));
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
        Ok(())
    }

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
        if manifest.bundled && (manifest.archive.is_some() || manifest.kind != ModelKind::Model) {
            return Err(format!("{id}: only a plain model can be bundled"));
        }
        let mut names = HashSet::new();
        if let Some(archive) = &manifest.archive {
            check_file(id, archive, revision, &mut names)?;
            if !archive.url.ends_with(&format!("/{}", archive.name)) {
                return Err(format!(
                    "{id}: the archive must be fetched under its own name"
                ));
            }
        }
        for file in manifest.files.iter() {
            check_file(id, file, revision, &mut names)?;
            let source = match &manifest.archive {
                // Unpacked files name the archive they come from.
                Some(archive) => file.url == archive.url,
                None => file.url.ends_with(&format!("/{}", file.name)),
            };
            if !source {
                return Err(format!(
                    "{id}: `{}` must be fetched from the pinned revision under its own name",
                    file.name
                ));
            }
        }
        for required in manifest.requires.iter() {
            if *required == manifest.id {
                return Err(format!("{id}: a manifest cannot require itself"));
            }
        }
        Ok(())
    }

    /// Requirements are registered, one level deep (a requirement requires nothing) and never bundled.
    fn check_requirements(models: &[ModelManifest]) -> Result<(), String> {
        for manifest in models {
            for required in manifest.requires.iter() {
                let Some(found) = find_in(models, required) else {
                    return Err(format!("{}: `{required}` is not registered", manifest.id));
                };
                if !found.requires.is_empty() || found.bundled {
                    return Err(format!(
                        "{}: `{required}` must be a downloadable manifest with no requirements",
                        manifest.id
                    ));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn every_manifest_follows_the_rules() {
        let mut ids = HashSet::new();
        for manifest in MODELS.iter().chain([&SAMPLE, &SAMPLE_RUNTIME]) {
            assert_eq!(check_manifest(manifest), Ok(()));
            assert!(ids.insert(manifest.id.as_str()), "duplicate model id");
        }
        assert_eq!(check_requirements(MODELS), Ok(()));
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
        let foreign_member = ModelManifest {
            files: StaticList::from(vec![ModelFile {
                url: StaticStr::new("https://github.com/org/tool/releases/download/v1/other.zip"),
                ..SAMPLE_RUNTIME_FILES[0].clone()
            }]),
            ..SAMPLE_RUNTIME.clone()
        };
        assert!(check_manifest(&foreign_member).is_err());
        let bundled_runtime = ModelManifest {
            bundled: true,
            ..SAMPLE_RUNTIME.clone()
        };
        assert!(check_manifest(&bundled_runtime).is_err());
        let needs_itself = ModelManifest {
            requires: StaticList::from(vec![SAMPLE.id.clone()]),
            ..SAMPLE.clone()
        };
        assert!(check_manifest(&needs_itself).is_err());
        let missing_requirement = ModelManifest {
            requires: StaticList::from(vec![ModelId::from_static("nowhere")]),
            ..SAMPLE.clone()
        };
        assert!(check_requirements(&[missing_requirement]).is_err());
    }

    #[test]
    fn an_install_set_lists_requirements_first_then_the_model() {
        let model = ModelManifest {
            requires: StaticList::from(vec![SAMPLE_RUNTIME.id.clone(), SAMPLE_RUNTIME.id.clone()]),
            ..SAMPLE.clone()
        };
        let models = [SAMPLE_RUNTIME.clone(), model.clone()];
        let set: Vec<&ModelId> = install_set_in(&models, &model)
            .unwrap()
            .into_iter()
            .map(|manifest| &manifest.id)
            .collect();
        assert_eq!(set, [&SAMPLE_RUNTIME.id, &SAMPLE.id]);
        assert_eq!(
            install_set_in(&[], &model)
                .err()
                .map(PortError::into_app_error),
            Some(AppError::NotFound {
                resource: ResourceKind::Model
            })
        );
        let qwen = find(&QWEN3_1_7B_Q4).unwrap();
        let ids: Vec<&ModelId> = install_set(qwen)
            .unwrap()
            .into_iter()
            .map(|manifest| &manifest.id)
            .collect();
        assert_eq!(ids, [&LLAMA_CPP_VULKAN, &QWEN3_1_7B_Q4]);
        assert_eq!(
            required_by(&LLAMA_CPP_VULKAN)
                .map(|manifest| &manifest.id)
                .collect::<Vec<_>>(),
            [&QWEN3_1_7B_Q4]
        );
        assert_eq!(required_by(&QWEN3_1_7B_Q4).count(), 0);
    }

    #[test]
    fn the_llm_and_its_runtime_are_pinned_downloads_of_the_documented_size() {
        let qwen = find(&QWEN3_1_7B_Q4).unwrap();
        assert!(!qwen.bundled);
        assert_eq!(qwen.kind, ModelKind::Model);
        assert_eq!(qwen.license.as_str(), "Apache-2.0");
        // 02 §2.5: about 1.3 GB.
        assert_eq!(qwen.total_bytes().get(), 1_282_439_264);
        let runtime = find(&LLAMA_CPP_VULKAN).unwrap();
        assert_eq!(runtime.kind, ModelKind::Runtime);
        assert_eq!(runtime.license.as_str(), "MIT");
        // 02 §2.5: a ~32 MB archive, about 86 MB unpacked.
        assert_eq!(runtime.transfer_bytes().get(), 32_127_004);
        assert_eq!(runtime.total_bytes().get(), 86_030_109);
        assert!(
            runtime
                .files
                .iter()
                .any(|file| file.name.as_str() == LLAMA_SERVER_EXE)
        );
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
