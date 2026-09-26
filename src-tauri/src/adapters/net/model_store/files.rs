/*!
 * SOURCE OF TRUTH KEYWORDS: model file digest, SHA-256 streaming, hash prefix, block copy, Progress reporter, ModelProgress emit, yield between blocks
 * WHAT:  What download, import and verify share: Progress (emits ModelProgress for one manifest with cumulative
 *        bytes over the operation's total), `hash_prefix` (SHA-256 of a file's first bytes, reporting as it goes),
 *        `copy_hashed` / `write_hashed` (copy a file, or any reader such as an archive member, while hashing it) and
 *        `digest_hex`/`matches` (compare with the manifest's digest).
 * WHY:   Every byte of a model is hashed exactly once per transfer: while it arrives, or while it is copied, or
 *        (on resume) the part already on disk (02 §8.2). Work is done in 1 MiB blocks on the calling task with a
 *        yield between blocks: a block takes about a millisecond, so the runtime's other tasks keep running, and
 *        cancelling (dropping the future) stops at the next block with no blocking thread left behind and no
 *        progress sink to smuggle across threads. Progress is reported per block; the pipeline throttles it to
 *        10 Hz (ports/model_store.rs).
 * WHERE: download.rs (resume prefix), copy.rs (import copy, verify), archive.rs (unpacking).
 */

use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use sha2::{Digest, Sha256};

use super::layout::storage;
use crate::{
    ports::EventSink,
    types::{ByteCount, ModelFile, ModelManifest, ModelPhase, ModelProgress, PortResult},
};

/// Bytes read, hashed or written per step.
pub(super) const BLOCK: usize = 1 << 20;

/// Reports one manifest's progress as cumulative bytes over the total the operation moves (the archive for an
/// archive download, the files for a file download, import or check).
pub(super) struct Progress<'a> {
    pub sink: &'a dyn EventSink<ModelProgress>,
    pub manifest: &'a ModelManifest,
    pub total: ByteCount,
}

impl Progress<'_> {
    pub fn report(&self, bytes: u64, phase: ModelPhase) {
        self.sink.emit(ModelProgress {
            model_id: self.manifest.id.clone(),
            bytes: ByteCount::new(bytes),
            total: self.total,
            phase,
        });
    }
}

/// Whether `hasher`'s digest is `file`'s manifest digest.
pub(super) fn matches(hasher: Sha256, file: &ModelFile) -> bool {
    digest_hex(hasher) == file.sha256.as_str()
}

/// The digest as 64 lowercase hex characters (the manifest's spelling).
pub(super) fn digest_hex(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// SHA-256 of the first `len` bytes of `path` (none read when `len` is 0); `on_block` gets the bytes hashed so far.
pub(super) async fn hash_prefix(
    path: &Path,
    len: u64,
    mut on_block: impl FnMut(u64),
) -> PortResult<Sha256> {
    let mut hasher = Sha256::new();
    if len == 0 {
        return Ok(hasher);
    }
    let mut source =
        File::open(path).map_err(|error| storage("opening a model file", path, &error))?;
    let mut buffer = vec![0_u8; BLOCK];
    let mut done = 0_u64;
    while done < len {
        let want = usize::try_from(len - done).map_or(BLOCK, |left| left.min(BLOCK));
        let slice = buffer.get_mut(..want).unwrap_or_default();
        source
            .read_exact(slice)
            .map_err(|error| storage("reading a model file", path, &error))?;
        hasher.update(&*slice);
        done += want as u64;
        on_block(done);
        tokio::task::yield_now().await;
    }
    Ok(hasher)
}

/// Copies `from` to a new file `to` while hashing it; `on_block` gets the bytes copied so far.
pub(super) async fn copy_hashed(
    from: &Path,
    to: &Path,
    on_block: impl FnMut(u64),
) -> PortResult<Sha256> {
    let mut source =
        File::open(from).map_err(|error| storage("opening a model file", from, &error))?;
    write_hashed(&mut source, from, to, u64::MAX, on_block).await
}

/**
 * SOURCE OF TRUTH KEYWORDS: write_hashed, stream to file with hash, bounded copy, archive member copy
 * WHAT:  Writes what `source` yields (named `from` in errors) to a new file `to` while hashing it, in blocks with a
 *        yield between them, and returns the hasher; it stops once more than `limit` bytes were written, leaving the
 *        oversized file for the caller's size or hash check to reject.
 * WHY:   A file copy and an archive member being unpacked are the same loop; the limit keeps a member that inflates
 *        past its manifest size from filling the disk.
 * WHERE: copy_hashed (import); archive.rs (unpacking a release archive).
 */
pub(super) async fn write_hashed(
    source: &mut impl Read,
    from: &Path,
    to: &Path,
    limit: u64,
    mut on_block: impl FnMut(u64),
) -> PortResult<Sha256> {
    let mut target =
        File::create(to).map_err(|error| storage("creating a model file", to, &error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; BLOCK];
    let mut done = 0_u64;
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|error| storage("reading a model file", from, &error))?;
        let Some(block) = buffer.get(..read).filter(|block| !block.is_empty()) else {
            break;
        };
        target
            .write_all(block)
            .map_err(|error| storage("writing a model file", to, &error))?;
        hasher.update(block);
        done += read as u64;
        on_block(done);
        if done > limit {
            break;
        }
        tokio::task::yield_now().await;
    }
    target
        .sync_all()
        .map_err(|error| storage("saving a model file", to, &error))?;
    Ok(hasher)
}
