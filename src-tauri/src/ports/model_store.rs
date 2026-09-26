/*!
 * SOURCE OF TRUTH KEYWORDS: ModelStore, model download, model import, verify model, remove model, model status, locate model, resume download
 * WHAT:  ModelStore installs, checks, finds and removes model files described by a ModelManifest: download
 *        (network), import (a folder on disk), verify (SHA-256), status, locate and remove.
 * WHY:   Adapters may not read the registry (02 §3.2), so every call takes the manifest. Transfers are async
 *        (BoxFuture) and cancelled by dropping the future; an adapter keeps the `.partial` folder intact on
 *        cancel so the next download resumes (02 §8.2). Adapters report the non-terminal phases (transferring,
 *        verifying, installing) as often as they like; the pipeline throttles to 10 Hz and emits the terminal
 *        phase, so rate policy lives in one place. Allowlisted hosts and offline mode are enforced before the
 *        call (factory permission) and again inside the one HTTP client (02 §10).
 * WHERE: Implemented by adapters/net/model_store.rs (HttpModelStore) and ports/fakes; orchestrated by
 *        pipeline/models.rs for the `models_*` commands, onboarding and engine loading.
 */

use std::path::{Path, PathBuf};

use super::EventSink;
use crate::types::{BoxFuture, ModelManifest, ModelProgress, ModelStatus, PortResult};

/// Local model files.
pub trait ModelStore: Send + Sync {
    /// Whether the model is installed, partly downloaded, damaged or absent (a cheap size check, no hashing): an
    /// install whose files all have the manifest's sizes is `Installed`, a download in progress `Partial`, an install
    /// with a missing or wrongly sized file `Corrupt`. A bundled model is always `Installed`.
    fn status(&self, manifest: &ModelManifest) -> PortResult<ModelStatus>;

    /// The folder holding the installed model, or None when it is not installed.
    fn locate(&self, manifest: &ModelManifest) -> PortResult<Option<PathBuf>>;

    /// Downloads every file, resuming a partial download, verifies each hash and installs atomically (replacing a
    /// damaged install). Fails with `Network` when a host cannot be reached or the connection drops (what arrived is
    /// kept for the next call), `Offline` when offline mode is switched on, and `ModelCorrupt`
    /// when a hash does not match (that file is discarded).
    fn download<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        progress: &'a dyn EventSink<ModelProgress>,
    ) -> BoxFuture<'a, PortResult<()>>;

    /// Copies the model from `source` (a folder the user picked), verifies it and installs it; works offline.
    /// Fails with `NotFound { model }` when a file is missing from `source` and `ModelCorrupt` when a file there has
    /// another size or hash (a different model or version).
    fn import<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        source: &'a Path,
        progress: &'a dyn EventSink<ModelProgress>,
    ) -> BoxFuture<'a, PortResult<()>>;

    /// Re-hashes an installed model. Fails with `ModelMissing` when it is not installed and `ModelCorrupt` when
    /// a file does not match.
    fn verify<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        progress: &'a dyn EventSink<ModelProgress>,
    ) -> BoxFuture<'a, PortResult<()>>;

    /// Deletes the installed model and any partial download. Removing an absent model is a no-op; a bundled
    /// model cannot be removed (`Validation`).
    fn remove(&self, manifest: &ModelManifest) -> PortResult<()>;
}
