/*!
 * SOURCE OF TRUTH KEYWORDS: ONNX Runtime loader, ensure_runtime, load-dynamic, LoadLibraryExW absolute path, DirectML preload, app-local VC runtime, ort init_from, telemetry off
 * WHAT:  `ensure_runtime` loads the bundled ONNX Runtime once per process: every file of ONNX_RUNTIME_LOAD_ORDER by
 *        absolute path (C++ runtime, DirectML, then onnxruntime.dll), then hands onnxruntime.dll to `ort` and
 *        commits the global environment with telemetry off.
 * WHY:   `ort` uses `load-dynamic`, so nothing is linked; left alone it would find `onnxruntime.dll` through the DLL
 *        search order, and this machine class has stale copies in System32 (05 A5, W25). onnxruntime.dll imports
 *        MSVCP140 and delay-loads DirectML.dll, and Windows resolves those by module name: loading Echo's copies
 *        first makes every later lookup find them (05 W32). A C++ runtime DLL the process already has loaded is
 *        left alone, because two copies of one runtime in a process are worse than an older one. Each library is
 *        loaded with LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | DEFAULT_DIRS, so its own imports resolve from the bundle and
 *        System32 only, never PATH or the working directory. Modules stay loaded for the life of the process (an
 *        ONNX session may exist until exit). ONNX Runtime's Windows build reports usage through ETW telemetry by
 *        default; Echo sends no telemetry (00 constraint 1), so the environment turns it off. The outcome is
 *        cached: a missing file cannot appear while Echo runs.
 * WHERE: Called by adapters/onnx/session.rs before any ONNX session is created (Silero VAD now, Parakeet next).
 */

use std::{path::Path, sync::OnceLock};

use windows::{
    Win32::System::LibraryLoader::{
        GetModuleHandleW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
        LoadLibraryExW,
    },
    core::HSTRING,
};

use crate::types::{AppError, AppPaths, ONNX_RUNTIME_LOAD_ORDER, PortError, PortResult};

/// The result of the one load attempt this process makes; the error is the log detail.
static RUNTIME: OnceLock<Result<(), String>> = OnceLock::new();

/// Name ONNX Runtime gives its logging environment.
const ENVIRONMENT_NAME: &str = "echo";

/// Loads the bundled ONNX Runtime if this process has not yet; fails with `Internal` when the bundle is broken.
pub fn ensure_runtime(paths: &AppPaths) -> PortResult<()> {
    match RUNTIME.get_or_init(|| load(paths)) {
        Ok(()) => Ok(()),
        Err(detail) => Err(PortError::new(AppError::Internal).with_detail(detail.clone())),
    }
}

fn load(paths: &AppPaths) -> Result<(), String> {
    let mut runtime_path = None;
    for library in ONNX_RUNTIME_LOAD_ORDER {
        let path = paths.onnx_runtime_file(library);
        if !path.is_file() {
            if library.required {
                return Err(format!("the bundled {} is missing", library.file_name));
            }
            tracing::info!(
                library = library.file_name,
                "optional ONNX Runtime library is not bundled"
            );
            continue;
        }
        if library.process_shared && is_loaded(library.file_name) {
            tracing::debug!(
                library = library.file_name,
                "the process already has this C++ runtime loaded"
            );
            continue;
        }
        load_library(&path)?;
        runtime_path = Some(path);
    }
    let runtime_path = runtime_path.ok_or("ONNX_RUNTIME_LOAD_ORDER lists no library")?;
    let committed = ort::init_from(&runtime_path)
        .map_err(|error| format!("ONNX Runtime did not initialise: {error}"))?
        .with_name(ENVIRONMENT_NAME)
        .with_telemetry(false)
        .commit();
    if !committed {
        tracing::warn!("an ONNX Runtime environment already existed; its settings stay in effect");
    }
    tracing::info!(
        api = ort::MINOR_VERSION,
        "ONNX Runtime loaded from the bundle"
    );
    Ok(())
}

fn is_loaded(file_name: &str) -> bool {
    // SAFETY: the name is a valid null-terminated HSTRING for the whole call; the handle is only tested for success
    // and never freed (GetModuleHandleW does not add a reference).
    unsafe { GetModuleHandleW(&HSTRING::from(file_name)) }.is_ok()
}

fn load_library(path: &Path) -> Result<(), String> {
    // SAFETY: the path is a valid null-terminated HSTRING for the whole call. The module is intentionally never
    // freed: ONNX Runtime and its sessions may use it until the process exits.
    unsafe {
        LoadLibraryExW(
            &HSTRING::from(path),
            None,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        )
    }
    .map(|_| ())
    .map_err(|error| {
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        format!("could not load the bundled {name}: {error}")
    })
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::types::testing::{TempDir, source_resource_paths};

    /// The bundle as recorded in docs/05 (ONNX Runtime 1.24.2 DirectML build, DirectML 1.15.4, VC++ 14.51).
    const BUNDLED: &[(&str, &str)] = &[
        (
            "vcruntime140.dll",
            "d1f4225df2cd877dbf130d5668a021dce3f94118455ff5ec952061c30afc9ce7",
        ),
        (
            "vcruntime140_1.dll",
            "a7146c08f89fe5b04541ab507cdb59ff7b44534d4ba3c668a426c6450a03434e",
        ),
        (
            "msvcp140.dll",
            "7c26614e1d733892c2deac7e245ce115504b1d80592dd0a01b08e3e5a55f89ca",
        ),
        (
            "msvcp140_1.dll",
            "206c931bf90fdad8816de3b5e2ef80b2bcaa9406c89ecc05fe6fddffe251e982",
        ),
        (
            "DirectML.dll",
            "9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1",
        ),
        (
            "onnxruntime.dll",
            "a2323bc49544645b911743052f1edce594e17df1e3423b71468c7386bc902f80",
        ),
    ];

    #[test]
    fn the_bundle_holds_every_pinned_library_in_load_order() {
        let data = TempDir::new("onnx-bundle");
        let paths = source_resource_paths(data.path());
        let names: Vec<_> = ONNX_RUNTIME_LOAD_ORDER
            .iter()
            .map(|library| library.file_name)
            .collect();
        let pinned: Vec<_> = BUNDLED.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, pinned);
        for (library, (_, sha256)) in ONNX_RUNTIME_LOAD_ORDER.iter().zip(BUNDLED) {
            let bytes = std::fs::read(paths.onnx_runtime_file(library)).unwrap();
            let digest: String = Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(&digest, sha256, "{}", library.file_name);
        }
    }

    #[test]
    fn the_runtime_loads_from_the_bundle_once() {
        let data = TempDir::new("onnx-load");
        let paths = source_resource_paths(data.path());
        ensure_runtime(&paths).unwrap();
        ensure_runtime(&paths).unwrap();
        assert!(is_loaded("onnxruntime.dll"));
    }

    #[test]
    fn a_broken_bundle_is_reported_not_loaded() {
        let empty = AppPaths::new(std::env::temp_dir(), std::env::temp_dir().join("no-bundle"));
        let error = load(&empty).unwrap_err();
        assert!(error.contains("vcruntime140.dll"), "{error}");
    }
}
