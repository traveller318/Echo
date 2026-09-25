/*!
 * SOURCE OF TRUTH KEYWORDS: FakeModelStore, fake model download, fake model import, partial download, corrupt model test, hanging download
 * WHAT:  FakeModelStore: an in-memory ModelStore with installed, partial and corrupt models, scripted failures and
 *        transfers that can hang (to test cancellation by dropping the future).
 * WHY:   Model manager, onboarding and engine-load tests need every path of 02 §8.2 (resume from partial, hash
 *        mismatch, offline import, missing model) without network or disk. Transfers report the same non-terminal
 *        phases a real store does, in order: transferring (from the resume point to the total), verifying,
 *        installing. `locate` returns `<root>/<model id>`, which never exists on disk.
 * WHERE: pipeline model orchestration, onboarding and ASR engine loading tests.
 */

use std::{
    collections::{HashMap, HashSet},
    future,
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::lock;
use crate::{
    ports::{EventSink, ModelStore},
    types::{
        AppError, BoxFuture, ByteCount, ModelId, ModelManifest, ModelPhase, ModelProgress,
        ModelStatus, PortError, PortResult,
    },
};

#[derive(Default)]
struct StoreState {
    installed: HashSet<ModelId>,
    partial: HashMap<ModelId, u64>,
    corrupt: HashSet<ModelId>,
    broken: HashSet<ModelId>,
    next_error: Option<PortError>,
    hang: bool,
}

/// An in-memory model store.
pub struct FakeModelStore {
    root: PathBuf,
    state: Mutex<StoreState>,
}

impl FakeModelStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            state: Mutex::default(),
        }
    }

    /// The model is already installed.
    pub fn install(&self, id: &ModelId) {
        lock(&self.state).installed.insert(id.clone());
    }

    /// An earlier download stopped after `bytes`.
    pub fn leave_partial(&self, id: &ModelId, bytes: u64) {
        lock(&self.state).partial.insert(id.clone(), bytes);
    }

    /// Every hash check of this model fails.
    pub fn corrupt(&self, id: &ModelId) {
        lock(&self.state).corrupt.insert(id.clone());
    }

    /// Hash checks of this model pass again (fresh files arrived).
    pub fn repair(&self, id: &ModelId) {
        lock(&self.state).corrupt.remove(id);
    }

    /// The installed files no longer have their sizes: `status` says `Corrupt` until a download or import.
    pub fn break_install(&self, id: &ModelId) {
        lock(&self.state).broken.insert(id.clone());
    }

    /// The next download, import or verify fails with `error`.
    pub fn fail_next(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }

    /// Downloads and imports never complete until set back to false.
    pub fn hang_transfers(&self, hang: bool) {
        lock(&self.state).hang = hang;
    }

    fn progress(manifest: &ModelManifest, bytes: u64, phase: ModelPhase) -> ModelProgress {
        ModelProgress {
            model_id: manifest.id.clone(),
            bytes: ByteCount::new(bytes),
            total: manifest.total_bytes(),
            phase,
        }
    }

    /// Download and import share this flow; `resume` says whether a partial download counts.
    fn transfer<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        progress: &'a dyn EventSink<ModelProgress>,
        resume: bool,
    ) -> BoxFuture<'a, PortResult<()>> {
        let mut state = lock(&self.state);
        if manifest.bundled {
            return Box::pin(future::ready(Err(AppError::validation(
                "model",
                "Bundled models ship with Echo.",
            )
            .into())));
        }
        if let Some(error) = state.next_error.take() {
            return Box::pin(future::ready(Err(error)));
        }
        if state.hang {
            return Box::pin(future::pending());
        }
        drop(state);
        Box::pin(async move {
            let total = manifest.total_bytes().get();
            let mut state = lock(&self.state);
            let start = if resume {
                state.partial.get(&manifest.id).copied().unwrap_or(0)
            } else {
                0
            };
            for (bytes, phase) in [
                (start, ModelPhase::Transferring),
                (total, ModelPhase::Transferring),
                (total, ModelPhase::Verifying),
            ] {
                progress.emit(Self::progress(manifest, bytes, phase));
            }
            state.partial.remove(&manifest.id);
            if state.corrupt.contains(&manifest.id) {
                return Err(AppError::ModelCorrupt {
                    model_id: manifest.id.clone(),
                }
                .into());
            }
            progress.emit(Self::progress(manifest, total, ModelPhase::Installing));
            state.installed.insert(manifest.id.clone());
            state.broken.remove(&manifest.id);
            Ok(())
        })
    }
}

impl ModelStore for FakeModelStore {
    fn status(&self, manifest: &ModelManifest) -> PortResult<ModelStatus> {
        let state = lock(&self.state);
        let installed = state.installed.contains(&manifest.id);
        Ok(
            if manifest.bundled || (installed && !state.broken.contains(&manifest.id)) {
                ModelStatus::Installed
            } else if let Some(&bytes) = state.partial.get(&manifest.id) {
                ModelStatus::Partial {
                    bytes: ByteCount::new(bytes),
                }
            } else if installed {
                ModelStatus::Corrupt
            } else {
                ModelStatus::NotInstalled
            },
        )
    }

    fn locate(&self, manifest: &ModelManifest) -> PortResult<Option<PathBuf>> {
        let installed = matches!(self.status(manifest)?, ModelStatus::Installed);
        Ok(installed.then(|| self.root.join(manifest.id.as_str())))
    }

    fn download<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        progress: &'a dyn EventSink<ModelProgress>,
    ) -> BoxFuture<'a, PortResult<()>> {
        self.transfer(manifest, progress, true)
    }

    fn import<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        _source: &'a Path,
        progress: &'a dyn EventSink<ModelProgress>,
    ) -> BoxFuture<'a, PortResult<()>> {
        self.transfer(manifest, progress, false)
    }

    fn verify<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        progress: &'a dyn EventSink<ModelProgress>,
    ) -> BoxFuture<'a, PortResult<()>> {
        Box::pin(async move {
            let mut state = lock(&self.state);
            if let Some(error) = state.next_error.take() {
                return Err(error);
            }
            if state.broken.contains(&manifest.id) {
                return Err(AppError::ModelCorrupt {
                    model_id: manifest.id.clone(),
                }
                .into());
            }
            if !manifest.bundled && !state.installed.contains(&manifest.id) {
                return Err(AppError::ModelMissing {
                    model_id: manifest.id.clone(),
                }
                .into());
            }
            let total = manifest.total_bytes().get();
            progress.emit(Self::progress(manifest, total, ModelPhase::Verifying));
            if state.corrupt.contains(&manifest.id) {
                return Err(AppError::ModelCorrupt {
                    model_id: manifest.id.clone(),
                }
                .into());
            }
            Ok(())
        })
    }

    fn remove(&self, manifest: &ModelManifest) -> PortResult<()> {
        if manifest.bundled {
            return Err(AppError::validation("model", "Bundled models ship with Echo.").into());
        }
        let mut state = lock(&self.state);
        state.installed.remove(&manifest.id);
        state.partial.remove(&manifest.id);
        state.broken.remove(&manifest.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::{
        ports::fakes::{RecordingSink, poll_once},
        types::{ModelFile, Sha256Hex, StaticList, StaticStr},
    };

    const FILES: &[ModelFile] = &[ModelFile {
        name: StaticStr::new("model.onnx"),
        url: StaticStr::new("https://huggingface.co/org/model/resolve/abc/model.onnx"),
        sha256: Sha256Hex::from_static(
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
        bytes: ByteCount::new(100),
    }];

    const MODEL: ModelManifest = ModelManifest {
        id: ModelId::from_static("fake-model"),
        label: StaticStr::new("Fake model"),
        license: StaticStr::new("MIT"),
        attribution: None,
        revision: StaticStr::new("abc"),
        files: StaticList::new(FILES),
        bundled: false,
    };

    fn phases(sink: &RecordingSink<ModelProgress>) -> Vec<(u64, ModelPhase)> {
        sink.events()
            .into_iter()
            .map(|progress| (progress.bytes.get(), progress.phase))
            .collect()
    }

    #[test]
    fn download_resumes_from_partial_and_installs() {
        let store = FakeModelStore::new("models");
        store.leave_partial(&MODEL.id, 40);
        assert_eq!(
            store.status(&MODEL).unwrap(),
            ModelStatus::Partial {
                bytes: ByteCount::new(40)
            }
        );
        assert_eq!(store.locate(&MODEL).unwrap(), None);
        let sink = RecordingSink::default();
        assert_eq!(
            poll_once(store.download(&MODEL, &sink)),
            Poll::Ready(Ok(()))
        );
        assert_eq!(
            phases(&sink),
            [
                (40, ModelPhase::Transferring),
                (100, ModelPhase::Transferring),
                (100, ModelPhase::Verifying),
                (100, ModelPhase::Installing),
            ]
        );
        assert_eq!(store.status(&MODEL).unwrap(), ModelStatus::Installed);
        assert_eq!(
            store.locate(&MODEL).unwrap(),
            Some(Path::new("models").join("fake-model"))
        );
    }

    #[test]
    fn corrupt_models_fail_download_and_verify() {
        let store = FakeModelStore::new("models");
        store.corrupt(&MODEL.id);
        let sink = RecordingSink::default();
        assert!(matches!(
            poll_once(store.import(&MODEL, Path::new("picked"), &sink)),
            Poll::Ready(Err(_))
        ));
        assert_eq!(store.status(&MODEL).unwrap(), ModelStatus::NotInstalled);
        assert!(matches!(
            poll_once(store.verify(&MODEL, &sink)),
            Poll::Ready(Err(_))
        ));
        store.install(&MODEL.id);
        assert_eq!(
            poll_once(store.verify(&MODEL, &sink))
                .map(|result| result.map_err(PortError::into_app_error)),
            Poll::Ready(Err(AppError::ModelCorrupt { model_id: MODEL.id }))
        );
    }

    #[test]
    fn a_hanging_download_keeps_the_partial_when_dropped() {
        let store = FakeModelStore::new("models");
        store.leave_partial(&MODEL.id, 10);
        store.hang_transfers(true);
        let sink = RecordingSink::default();
        assert!(poll_once(store.download(&MODEL, &sink)).is_pending());
        assert_eq!(
            store.status(&MODEL).unwrap(),
            ModelStatus::Partial {
                bytes: ByteCount::new(10)
            }
        );
        store.hang_transfers(false);
        store.fail_next(AppError::Network.into());
        assert!(matches!(
            poll_once(store.download(&MODEL, &sink)),
            Poll::Ready(Err(_))
        ));
    }

    #[test]
    fn a_broken_install_is_corrupt_until_downloaded_again() {
        let store = FakeModelStore::new("models");
        store.install(&MODEL.id);
        store.break_install(&MODEL.id);
        assert_eq!(store.status(&MODEL).unwrap(), ModelStatus::Corrupt);
        assert_eq!(store.locate(&MODEL).unwrap(), None);
        let sink = RecordingSink::default();
        assert_eq!(
            poll_once(store.verify(&MODEL, &sink))
                .map(|result| result.map_err(PortError::into_app_error)),
            Poll::Ready(Err(AppError::ModelCorrupt {
                model_id: MODEL.id.clone()
            }))
        );
        assert_eq!(
            poll_once(store.download(&MODEL, &sink)),
            Poll::Ready(Ok(()))
        );
        assert_eq!(store.status(&MODEL).unwrap(), ModelStatus::Installed);
    }

    #[test]
    fn remove_is_idempotent_and_bundled_models_stay() {
        let store = FakeModelStore::new("models");
        store.install(&MODEL.id);
        store.remove(&MODEL).unwrap();
        store.remove(&MODEL).unwrap();
        assert_eq!(store.status(&MODEL).unwrap(), ModelStatus::NotInstalled);
        let bundled = ModelManifest {
            bundled: true,
            ..MODEL
        };
        assert_eq!(store.status(&bundled).unwrap(), ModelStatus::Installed);
        assert!(store.remove(&bundled).is_err());
        let sink = RecordingSink::default();
        assert!(matches!(
            poll_once(store.download(&bundled, &sink)),
            Poll::Ready(Err(_))
        ));
    }
}
