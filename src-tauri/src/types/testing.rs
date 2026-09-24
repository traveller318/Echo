/*!
 * SOURCE OF TRUTH KEYWORDS: TempDir, test temp folder, source_resource_paths, bundled resources in tests, cfg(test) helper
 * WHAT:  TempDir: a fresh, uniquely named folder under the OS temp dir for one test, removed when dropped;
 *        `source_resource_paths`: AppPaths whose resources dir is the repository's `src-tauri/resources`.
 * WHY:   Database, journal, logging and adapter tests all need real files without touching the user's data;
 *        one helper keeps the naming (unique per call, so parallel tests never share a folder) and the cleanup in
 *        one place. The bundle maps every resource to the same relative path it has in `src-tauri/resources`, so
 *        tests of bundled files (ONNX Runtime, Silero) read the source folder exactly as the app reads the installed
 *        one. It lives in types/ because that is the one layer every other layer's tests may import
 *        (02 §3.2); it is `#[cfg(test)]`, so it never ships.
 * WHERE: `use crate::types::testing::TempDir` from tests in services/, pipeline/, adapters/ and app/.
 */

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{AppPaths, TranscriptId};

/// A per-test folder, removed with everything in it when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    /// Creates `echo-<label>-<unique>` under the OS temp dir.
    pub fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("echo-{label}-{}", TranscriptId::generate()));
        fs::create_dir_all(&path).unwrap_or_else(|error| panic!("temp dir {path:?}: {error}"));
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// A path inside the folder.
    pub fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// AppPaths over `data` whose resources are the repository's `src-tauri/resources` folder.
pub fn source_resource_paths(data: &Path) -> AppPaths {
    AppPaths::new(
        data,
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources"),
    )
}
