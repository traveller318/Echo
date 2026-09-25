/*!
 * SOURCE OF TRUTH KEYWORDS: import model from folder, offline import, verify model hashes, hash check on demand, ModelCorrupt, ModelMissing
 * WHAT:  `import`: copies a manifest's files from a folder the user picked into `models/<id>.partial/`, hashing
 *        them as they are copied, then installs the folder; `verify`: re-hashes an installed (or bundled) model.
 * WHY:   02 §8.2: import works fully offline and is held to the same SHA-256 as a download, so a file from another
 *        model or version can never be installed. The source folder is checked (every file present with its size)
 *        before the partial download is cleared, so picking the wrong folder costs nothing; picking the staging
 *        folder itself is refused, because clearing it would delete what is being imported. A missing file is
 *        `NotFound { model }` (the folder is not this model), a wrong size or hash is `ModelCorrupt`. Verify
 *        reports `Verifying` progress over the whole model and fails on the first mismatch.
 * WHERE: HttpModelStore::import and ::verify (mod.rs).
 */

use std::{fs, path::Path};

use super::{
    files::{Progress, copy_hashed, hash_prefix, matches},
    layout::{self, file_len, remove_file, storage},
};
use crate::types::{
    AppError, AppPaths, ModelFile, ModelManifest, ModelPhase, PortError, PortResult, ResourceKind,
};

/// Copies `manifest` from `source`, verifying every file, and installs it.
pub(super) async fn import(
    paths: &AppPaths,
    manifest: &ModelManifest,
    source: &Path,
    progress: &Progress<'_>,
) -> PortResult<()> {
    if manifest.bundled {
        return Err(layout::bundled());
    }
    let staging = paths.model_partial_dir(&manifest.id);
    if same_folder(source, &staging) {
        return Err(AppError::validation(
            "folder",
            "Pick the folder that holds the model files, not Echo's download folder.",
        )
        .into());
    }
    for file in manifest.files.iter() {
        match file_len(&source.join(file.name.as_str()))? {
            None => {
                return Err(PortError::new(AppError::NotFound {
                    resource: ResourceKind::Model,
                })
                .with_detail(format!("{} is not in the chosen folder", file.name)));
            }
            Some(bytes) if bytes != file.bytes.get() => {
                return Err(corrupt(
                    manifest,
                    file,
                    "has another size than the manifest",
                ));
            }
            Some(_) => {}
        }
    }
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|error| storage("clearing the download folder", &staging, &error))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|error| storage("creating the download folder", &staging, &error))?;
    let mut done = 0_u64;
    for file in manifest.files.iter() {
        let target = staging.join(file.name.as_str());
        let hasher = copy_hashed(&source.join(file.name.as_str()), &target, |bytes| {
            progress.report(done + bytes, ModelPhase::Transferring);
        })
        .await?;
        if !matches(hasher, file) {
            remove_file(&target)?;
            return Err(corrupt(manifest, file, "does not match its SHA-256"));
        }
        done += file.bytes.get();
    }
    progress.report(done, ModelPhase::Verifying);
    progress.report(done, ModelPhase::Installing);
    layout::install(paths, manifest)
}

/// Re-hashes every file of the installed (or bundled) model.
pub(super) async fn verify(
    paths: &AppPaths,
    manifest: &ModelManifest,
    progress: &Progress<'_>,
) -> PortResult<()> {
    let dir = layout::model_dir(paths, manifest);
    if !manifest.bundled && !dir.is_dir() {
        return Err(AppError::ModelMissing {
            model_id: manifest.id.clone(),
        }
        .into());
    }
    let mut done = 0_u64;
    progress.report(done, ModelPhase::Verifying);
    for file in manifest.files.iter() {
        let path = dir.join(file.name.as_str());
        let expected = file.bytes.get();
        if file_len(&path)? != Some(expected) {
            return Err(corrupt(manifest, file, "is missing or has another size"));
        }
        let hasher = hash_prefix(&path, expected, |bytes| {
            progress.report(done + bytes, ModelPhase::Verifying);
        })
        .await?;
        if !matches(hasher, file) {
            return Err(corrupt(manifest, file, "does not match its SHA-256"));
        }
        done += expected;
    }
    Ok(())
}

fn corrupt(manifest: &ModelManifest, file: &ModelFile, why: &str) -> PortError {
    PortError::new(AppError::ModelCorrupt {
        model_id: manifest.id.clone(),
    })
    .with_detail(format!("{} {why}", file.name))
}

/// Whether two paths name the same existing folder (a path that cannot be resolved is not the same).
fn same_folder(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}
