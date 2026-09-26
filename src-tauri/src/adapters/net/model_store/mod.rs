/*!
 * SOURCE OF TRUTH KEYWORDS: HttpModelStore, ModelStore adapter, model manager storage, download verify import remove, models folder
 * WHAT:  HttpModelStore: the ModelStore over the allowlisted HttpClient and the `models/` and `runtimes/` folders
 *        of AppPaths. `status`/`locate`/`remove` are on-disk (layout.rs), `download` fetches with resume
 *        (download.rs), `import` and `verify` copy and hash (copy.rs), a release archive is unpacked by
 *        archive.rs; files.rs holds what they share.
 * WHY:   02 §8.2 in one adapter behind the port, so the pipeline never touches a model file or a URL. Split by
 *        responsibility (the on-disk layout, the network path, the local path) like the other multi-part adapters.
 *        Model folders are exactly `AppPaths::install_dir` (`model_dir` for models), the folder engines load from
 *        (pipeline/asr load_request, the LLM polisher's build fn).
 * WHERE: Built by app/bootstrap; held by pipeline/models (ModelManager) as `Arc<dyn ModelStore>`.
 */

mod archive;
mod copy;
mod download;
mod files;
mod layout;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use files::Progress;

use super::http_client::HttpClient;
use crate::{
    ports::{EventSink, ModelStore},
    types::{AppPaths, BoxFuture, ModelManifest, ModelProgress, ModelStatus, PortResult},
};

/// Models on disk under `models/`, downloaded through the allowlisted HTTP client.
pub struct HttpModelStore {
    http: HttpClient,
    paths: AppPaths,
}

impl HttpModelStore {
    pub fn new(http: HttpClient, paths: AppPaths) -> Self {
        Self { http, paths }
    }
}

impl ModelStore for HttpModelStore {
    fn status(&self, manifest: &ModelManifest) -> PortResult<ModelStatus> {
        layout::status(&self.paths, manifest)
    }

    fn locate(&self, manifest: &ModelManifest) -> PortResult<Option<PathBuf>> {
        layout::locate(&self.paths, manifest)
    }

    fn download<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        progress: &'a dyn EventSink<ModelProgress>,
    ) -> BoxFuture<'a, PortResult<()>> {
        Box::pin(async move {
            let progress = Progress {
                sink: progress,
                manifest,
                total: manifest.transfer_bytes(),
            };
            download::download(&self.http, &self.paths, manifest, &progress).await
        })
    }

    fn import<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        source: &'a Path,
        progress: &'a dyn EventSink<ModelProgress>,
    ) -> BoxFuture<'a, PortResult<()>> {
        Box::pin(copy::import(&self.paths, manifest, source, progress))
    }

    fn verify<'a>(
        &'a self,
        manifest: &'a ModelManifest,
        progress: &'a dyn EventSink<ModelProgress>,
    ) -> BoxFuture<'a, PortResult<()>> {
        Box::pin(async move {
            let progress = Progress {
                sink: progress,
                manifest,
                total: manifest.total_bytes(),
            };
            copy::verify(&self.paths, manifest, &progress).await
        })
    }

    fn remove(&self, manifest: &ModelManifest) -> PortResult<()> {
        layout::remove(&self.paths, manifest)
    }
}
