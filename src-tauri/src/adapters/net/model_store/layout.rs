/*!
 * SOURCE OF TRUTH KEYWORDS: model folder layout, model status size check, install model atomically, remove model, partial folder, removing folder, Corrupt status
 * WHAT:  The on-disk side of the model store (models in `models/`, runtimes in `runtimes/`, by ModelKind):
 *        `status` (the cheap size check), `locate`, `install` (the verified
 *        `.partial` folder renamed into place, replacing an old install) and `remove`, plus small file helpers
 *        shared by download and import.
 * WHY:   02 §8.2: files are staged in `models/<id>.partial/` and become the model in one rename, so a model folder
 *        is either complete or absent. A folder being replaced or removed is first renamed to `<id>.removing/`
 *        (AppPaths), so the model disappears atomically too, and a delete that fails half-way (antivirus holding a
 *        file) leaves only a leftover that the next install or removal clears. Status is by size only; hashes are
 *        checked on demand (verify) or after a failed load (02 §8.2). An install folder with a missing or wrongly
 *        sized file is `Corrupt`, not "not installed", so the page offers a fresh download instead of hiding it;
 *        a download in progress wins over it (`Partial`), because that is what the next click resumes. A manifest
 *        that arrives as an archive keeps only the archive in its `.partial` folder until it is unpacked, so that
 *        file's length is what a resume keeps.
 * WHERE: HttpModelStore (mod.rs), download.rs and copy.rs.
 */

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::types::{
    AppError, AppPaths, ByteCount, ModelManifest, ModelStatus, PortError, PortResult,
};

/// The folder that holds `manifest`'s files when it is usable: the resources folder for a bundled model.
pub(super) fn model_dir(paths: &AppPaths, manifest: &ModelManifest) -> PathBuf {
    if manifest.bundled {
        paths.bundled_models_dir()
    } else {
        paths.install_dir(manifest.kind, &manifest.id)
    }
}

/// Installed, partial, corrupt or absent, from file sizes alone.
pub(super) fn status(paths: &AppPaths, manifest: &ModelManifest) -> PortResult<ModelStatus> {
    if manifest.bundled {
        return Ok(ModelStatus::Installed);
    }
    let installed = paths.install_dir(manifest.kind, &manifest.id);
    let install_exists = is_dir(&installed)?;
    if install_exists && sizes_match(&installed, manifest)? {
        return Ok(ModelStatus::Installed);
    }
    let partial = paths.install_partial_dir(manifest.kind, &manifest.id);
    if is_dir(&partial)? {
        return Ok(ModelStatus::Partial {
            bytes: ByteCount::new(partial_bytes(&partial, manifest)?),
        });
    }
    Ok(if install_exists {
        ModelStatus::Corrupt
    } else {
        ModelStatus::NotInstalled
    })
}

/// The folder of the installed model, or None when it is not installed as its manifest says.
pub(super) fn locate(paths: &AppPaths, manifest: &ModelManifest) -> PortResult<Option<PathBuf>> {
    Ok(status(paths, manifest)?
        .is_installed()
        .then(|| model_dir(paths, manifest)))
}

/**
 * SOURCE OF TRUTH KEYWORDS: install model, atomic rename, replace damaged install, partial to model folder
 * WHAT:  Moves the verified `.partial` folder to `models/<id>/`; an existing install is moved aside first and
 *        deleted after.
 * WHY:   Windows cannot rename a folder over another one, so the old install goes to `.removing` (one rename, so
 *        the model is never half-replaced) and the new one takes its place; the old copy's delete is best effort.
 * WHERE: download.rs and copy.rs (import) once every file is verified.
 */
pub(super) fn install(paths: &AppPaths, manifest: &ModelManifest) -> PortResult<()> {
    let target = paths.install_dir(manifest.kind, &manifest.id);
    let removing = paths.install_removal_dir(manifest.kind, &manifest.id);
    if is_dir(&target)? {
        move_aside(&target, &removing, "replacing the old model")?;
    }
    fs::rename(
        paths.install_partial_dir(manifest.kind, &manifest.id),
        &target,
    )
    .map_err(|error| storage("installing the model", &target, &error))?;
    clear_leftover(&removing);
    Ok(())
}

/// Deletes the installed model and any partial download; an absent model is already removed.
pub(super) fn remove(paths: &AppPaths, manifest: &ModelManifest) -> PortResult<()> {
    if manifest.bundled {
        return Err(bundled());
    }
    let target = paths.install_dir(manifest.kind, &manifest.id);
    let removing = paths.install_removal_dir(manifest.kind, &manifest.id);
    if is_dir(&target)? {
        move_aside(&target, &removing, "removing the model")?;
        clear_leftover(&removing);
    }
    let partial = paths.install_partial_dir(manifest.kind, &manifest.id);
    if is_dir(&partial)? {
        fs::remove_dir_all(&partial)
            .map_err(|error| storage("removing the partial download", &partial, &error))?;
    }
    Ok(())
}

/// The error for a transfer or removal asked of a model that ships with Echo.
pub(super) fn bundled() -> PortError {
    AppError::validation("model", "This model ships with Echo.").into()
}

/// The length of `path`, or None when it does not exist.
pub(super) fn file_len(path: &Path) -> PortResult<Option<u64>> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(metadata.len())),
        Ok(_) => Err(PortError::new(AppError::Storage)
            .with_detail(format!("{} is not a file", path.display()))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(storage("reading a model file", path, &error)),
    }
}

/// Deletes `path` if it exists; a file that cannot be deleted is a Storage error.
pub(super) fn remove_file(path: &Path) -> PortResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(storage("deleting a model file", path, &error)),
    }
}

/// A local file operation failed: `Storage`, with the path and cause for the log.
pub(super) fn storage(what: &str, path: &Path, error: &io::Error) -> PortError {
    PortError::new(AppError::Storage).with_detail(format!("{what} ({}): {error}", path.display()))
}

fn is_dir(path: &Path) -> PortResult<bool> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(storage("reading the models folder", path, &error)),
    }
}

/// Every manifest file is in `dir` with its exact size.
fn sizes_match(dir: &Path, manifest: &ModelManifest) -> PortResult<bool> {
    for file in manifest.files.iter() {
        if file_len(&dir.join(file.name.as_str()))? != Some(file.bytes.get()) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Bytes a resumed download would keep: the archive's length, or each file's, capped at its manifest size.
fn partial_bytes(dir: &Path, manifest: &ModelManifest) -> PortResult<u64> {
    if let Some(archive) = &manifest.archive {
        return Ok(file_len(&dir.join(archive.name.as_str()))?
            .unwrap_or(0)
            .min(archive.bytes.get()));
    }
    manifest.files.iter().try_fold(0_u64, |total, file| {
        let kept = file_len(&dir.join(file.name.as_str()))?
            .unwrap_or(0)
            .min(file.bytes.get());
        Ok(total.saturating_add(kept))
    })
}

/// Takes `target` out of the way in one rename to `removing` (an old leftover is cleared first); when a leftover
/// that cannot be deleted blocks the rename, `target` is deleted in place instead.
fn move_aside(target: &Path, removing: &Path, what: &str) -> PortResult<()> {
    clear_leftover(removing);
    if fs::rename(target, removing).is_ok() {
        return Ok(());
    }
    fs::remove_dir_all(target).map_err(|error| storage(what, target, &error))
}

/// Best-effort delete of a `.removing` leftover; one that cannot be deleted now is tried again next time.
fn clear_leftover(removing: &Path) {
    match fs::remove_dir_all(removing) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(
            folder = %removing.display(),
            %error,
            "an old model folder could not be deleted yet"
        ),
    }
}
