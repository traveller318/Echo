/*!
 * SOURCE OF TRUTH KEYWORDS: TempDir, test temp folder, source_resource_paths, installed_app_paths, installed_resource_files, dev model install, bundled resources in tests, cfg(test) helper
 * WHAT:  TempDir: a fresh, uniquely named folder under the OS temp dir for one test, removed when dropped;
 *        `source_resource_paths`: AppPaths whose resources dir is the repository's `src-tauri/resources`;
 *        `installed_app_paths`: the same, over this machine's real Echo data folder (where downloaded models live);
 *        `installed_resource_files`: every file tauri.conf.json's `bundle.resources` installs, by installed path.
 * WHY:   Database, journal, logging and adapter tests all need real files without touching the user's data;
 *        one helper keeps the naming (unique per call, so parallel tests never share a folder) and the cleanup in
 *        one place. The bundle maps every resource to the same relative path it has in `src-tauri/resources`, so
 *        tests of bundled files (ONNX Runtime, Silero) read the source folder exactly as the app reads the installed
 *        one. Downloaded models (Parakeet, 670 MB) cannot live in the repository, so the tests that need one read the
 *        developer's installed copy, read-only, from the folder Tauri's `app_local_data_dir()` resolves to (the
 *        local app data folder plus the identifier in tauri.conf.json); the step 10 dev install puts it there.
 *        It lives in types/ because that is the one layer every other layer's tests may import
 *        (02 §3.2); it is `#[cfg(test)]`, so it never ships.
 * WHERE: `use crate::types::testing::TempDir` from tests in services/, pipeline/, adapters/ and app/.
 */

use std::{
    collections::BTreeSet,
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

/// The tauri.conf.json this build runs on: the shared file with this operating system's file merged over it, the way
/// Tauri's own configuration loader does (JSON Merge Patch: objects merge key by key, other values replace).
pub fn build_config() -> serde_json::Value {
    #[cfg(windows)]
    const PLATFORM: &str = include_str!("../../tauri.windows.conf.json");
    #[cfg(target_os = "linux")]
    const PLATFORM: &str = include_str!("../../tauri.linux.conf.json");
    fn merge(base: &mut serde_json::Value, patch: serde_json::Value) {
        match (base, patch) {
            (serde_json::Value::Object(base), serde_json::Value::Object(patch)) => {
                for (key, value) in patch {
                    merge(base.entry(key).or_insert(serde_json::Value::Null), value);
                }
            }
            (base, patch) => *base = patch,
        }
    }
    let mut config: serde_json::Value =
        serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
    merge(&mut config, serde_json::from_str(PLATFORM).unwrap());
    config
}

/// This operating system's per-user data root, where Tauri's `app_local_data_dir()` puts the app folder.
fn local_data_root() -> PathBuf {
    #[cfg(windows)]
    return PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());
    #[cfg(target_os = "linux")]
    return std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/share"));
}

/// AppPaths over this machine's installed Echo data folder (read-only use: downloaded models) with the repository's
/// resources; the same data folder the app resolves through `app_local_data_dir()`.
pub fn installed_app_paths() -> AppPaths {
    let config = build_config();
    let identifier = config["identifier"].as_str().unwrap();
    source_resource_paths(&local_data_root().join(identifier))
}

/// Every file the installer puts in the resources folder, as `folder/file` with forward slashes, read from
/// the `bundle.resources` map of the build's configuration: a source whose last part holds a `*` (`dir/*`,
/// `dir/*.dll`) installs each matching file in `dir` under its target folder, any other source installs one file at
/// its target path.
pub fn installed_resource_files() -> BTreeSet<String> {
    let config = build_config();
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut installed = BTreeSet::new();
    for (source, target) in config["bundle"]["resources"].as_object().unwrap() {
        let target = target.as_str().unwrap();
        let (folder, pattern) = source.rsplit_once('/').unwrap_or((".", source));
        if let Some((prefix, suffix)) = pattern.split_once('*') {
            for entry in fs::read_dir(manifest_dir.join(folder)).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type().unwrap().is_file()
                    && name.len() >= prefix.len() + suffix.len()
                    && name.starts_with(prefix)
                    && name.ends_with(suffix)
                {
                    installed.insert(format!("{}/{name}", target.trim_end_matches('/')));
                }
            }
        } else {
            assert!(
                manifest_dir.join(source).is_file(),
                "{source} is not a file"
            );
            installed.insert(target.to_owned());
        }
    }
    installed
}
