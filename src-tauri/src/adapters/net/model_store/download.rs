/*!
 * SOURCE OF TRUTH KEYWORDS: model download, Range resume, resume partial download, SHA-256 per file, download_file, truncated download, server ignores range
 * WHAT:  `download`: fetches every file of a manifest (or its one archive) into its `.partial` folder, resuming each
 *        from the bytes already there, checks each file's SHA-256 as it arrives, unpacks an archive's listed files
 *        (archive.rs, each checked again) and deletes the archive, then installs the folder (layout::install).
 * WHY:   02 §8.2. A resumed file is re-hashed from disk first (the hash state cannot be saved), then only the rest
 *        is requested with `Range`; a file already complete is only checked. A server that ignores the range
 *        (200) restarts that file, a 416 (the local copy is longer than the file) discards it and starts over.
 *        What arrived stays on disk when the connection drops, the future is dropped (cancel) or offline mode is
 *        switched on, so the next call resumes; a file whose hash does not match, or that grows past its manifest
 *        size, is deleted and fails the call with `ModelCorrupt` (it is the wrong file, re-reading it cannot
 *        help). Writes go through a BufWriter on the calling task: a chunk lands in the OS cache in microseconds,
 *        and whatever is buffered when the future is dropped is flushed, then re-hashed on resume anyway.
 * WHERE: HttpModelStore::download (mod.rs).
 */

use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};

use sha2::{Digest, Sha256};

use super::{
    archive,
    files::{BLOCK, Progress, hash_prefix, matches},
    layout::{self, file_len, remove_file, storage},
};
use crate::{
    adapters::net::http_client::{HttpBody, HttpClient, HttpFetch},
    types::{AppError, AppPaths, ModelFile, ModelManifest, ModelPhase, PortError, PortResult},
};

/// Downloads, verifies and installs every file of `manifest` (from its archive when it has one).
pub(super) async fn download(
    http: &HttpClient,
    paths: &AppPaths,
    manifest: &ModelManifest,
    progress: &Progress<'_>,
) -> PortResult<()> {
    if manifest.bundled {
        return Err(layout::bundled());
    }
    let staging = paths.install_partial_dir(manifest.kind, &manifest.id);
    std::fs::create_dir_all(&staging)
        .map_err(|error| storage("creating the download folder", &staging, &error))?;
    let done = match &manifest.archive {
        None => {
            let mut done = 0_u64;
            for file in manifest.files.iter() {
                done = download_file(http, &staging, manifest, file, done, progress).await?;
            }
            // Each digest was checked as its file completed.
            progress.report(done, ModelPhase::Verifying);
            done
        }
        Some(archive) => {
            let done = download_file(http, &staging, manifest, archive, 0, progress).await?;
            // The archive's digest was checked as it completed; each unpacked file is checked against its own.
            progress.report(done, ModelPhase::Verifying);
            let packed = staging.join(archive.name.as_str());
            archive::unpack(&packed, &staging, manifest, |_| {}).await?;
            remove_file(&packed)?;
            done
        }
    };
    progress.report(done, ModelPhase::Installing);
    layout::install(paths, manifest)
}

/**
 * SOURCE OF TRUTH KEYWORDS: download one model file, resume offset, hash while downloading, early end
 * WHAT:  Brings `file` in `staging` to its full, verified content; returns the cumulative byte count after it
 *        (`done_before` plus its size).
 * WHY:   See the file header; each step keeps the invariant "the file on disk is a prefix of the right file".
 * WHERE: `download`.
 */
async fn download_file(
    http: &HttpClient,
    staging: &Path,
    manifest: &ModelManifest,
    file: &ModelFile,
    done_before: u64,
    progress: &Progress<'_>,
) -> PortResult<u64> {
    let path = staging.join(file.name.as_str());
    let expected = file.bytes.get();
    let mut kept = file_len(&path)?.unwrap_or(0);
    if kept > expected {
        remove_file(&path)?;
        kept = 0;
    }
    let mut hasher = hash_prefix(&path, kept, |bytes| {
        progress.report(done_before + bytes, ModelPhase::Transferring);
    })
    .await?;
    if kept == expected {
        if matches(hasher, file) {
            return Ok(done_before + expected);
        }
        remove_file(&path)?;
        kept = 0;
        hasher = Sha256::new();
    }
    let mut body = fetch(http, file, &path, &mut kept, &mut hasher).await?;
    if body.start() != kept {
        // The server sent the whole file: what was kept is replaced, not appended to.
        kept = 0;
        hasher = Sha256::new();
    }
    let target = Target {
        path: &path,
        kept,
        expected,
        manifest,
    };
    let written = receive(&mut body, &target, &mut hasher, |bytes| {
        progress.report(done_before + bytes, ModelPhase::Transferring);
    })
    .await?;
    if written < expected {
        return Err(PortError::new(AppError::Network).with_detail(format!(
            "{} ended early at {written} of {expected} bytes",
            file.name
        )));
    }
    if !matches(hasher, file) {
        remove_file(&path)?;
        return Err(PortError::new(AppError::ModelCorrupt {
            model_id: manifest.id.clone(),
        })
        .with_detail(format!("{} does not match its SHA-256", file.name)));
    }
    Ok(done_before + expected)
}

/// Requests `file` from `kept`; after a 416 the kept bytes are discarded and the whole file is requested.
async fn fetch(
    http: &HttpClient,
    file: &ModelFile,
    path: &Path,
    kept: &mut u64,
    hasher: &mut Sha256,
) -> PortResult<HttpBody> {
    match http.get_from(file.url.as_str(), *kept).await? {
        HttpFetch::Body(body) => Ok(body),
        HttpFetch::RangeNotSatisfiable => {
            remove_file(path)?;
            *kept = 0;
            *hasher = Sha256::new();
            match http.get_from(file.url.as_str(), 0).await? {
                HttpFetch::Body(body) => Ok(body),
                HttpFetch::RangeNotSatisfiable => Err(PortError::new(AppError::Network)
                    .with_detail(format!("the server refused {} from its start", file.name))),
            }
        }
    }
}

/// The file a body is written to.
struct Target<'a> {
    path: &'a Path,
    /// Bytes already on disk that the body continues (0: the body replaces the file).
    kept: u64,
    /// The manifest size of the file.
    expected: u64,
    manifest: &'a ModelManifest,
}

/**
 * SOURCE OF TRUTH KEYWORDS: receive body, append chunks, hash chunks, flush and sync
 * WHAT:  Appends `body` to the target file (truncating it first when `kept` is 0), hashing every byte; returns the
 *        file length reached. More bytes than `expected` deletes the file and fails with `ModelCorrupt`.
 * WHY:   The BufWriter is flushed and synced before the length is trusted, so an install never renames a file
 *        whose tail is still in a buffer.
 * WHERE: download_file.
 */
async fn receive(
    body: &mut HttpBody,
    target: &Target<'_>,
    hasher: &mut Sha256,
    mut on_bytes: impl FnMut(u64),
) -> PortResult<u64> {
    let Target {
        path,
        kept,
        expected,
        manifest,
    } = *target;
    let file = if kept == 0 {
        File::create(path)
    } else {
        OpenOptions::new().append(true).open(path)
    }
    .map_err(|error| storage("opening a model file", path, &error))?;
    let mut out = BufWriter::with_capacity(BLOCK, file);
    let mut written = kept;
    on_bytes(written);
    while let Some(chunk) = body.chunk().await? {
        let bytes = chunk.as_ref();
        let length = bytes.len() as u64;
        if written.saturating_add(length) > expected {
            drop(out);
            remove_file(path)?;
            return Err(PortError::new(AppError::ModelCorrupt {
                model_id: manifest.id.clone(),
            })
            .with_detail("the server sent more bytes than the manifest lists"));
        }
        out.write_all(bytes)
            .map_err(|error| storage("writing a model file", path, &error))?;
        hasher.update(bytes);
        written += length;
        on_bytes(written);
    }
    out.into_inner()
        .map_err(|error| storage("writing a model file", path, error.error()))?
        .sync_all()
        .map_err(|error| storage("saving a model file", path, &error))?;
    Ok(written)
}
