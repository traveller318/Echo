/*!
 * SOURCE OF TRUTH KEYWORDS: unpack archive, zip extraction, release archive, archive members, kept files, member SHA-256, zip bomb guard
 * WHAT:  `unpack`: takes the files a manifest lists out of its verified archive (a zip) into the staging folder,
 *        checking each against its own size and SHA-256; everything else in the archive is left behind.
 * WHY:   A runtime ships as one release zip (the llama.cpp release, 02 §2.5) but only some of its files are needed,
 *        and those are what the install folder keeps, so status, verify and remove work on them exactly like on a
 *        model's files (02 §8.2). Members are looked up by the manifest's plain file names only, so an entry path in
 *        the archive can never write outside the staging folder, and a member whose declared or inflated size
 *        differs from the manifest is refused before it can fill the disk. The archive was already hashed, so a bad
 *        member means a wrong manifest or a damaged file: `ModelCorrupt`. Work runs in blocks with a yield between
 *        them on the calling task, like every other store operation (files.rs).
 * WHERE: download.rs and copy.rs (an import from a folder holding the archive) after the archive is verified.
 */

use std::{fs::File, io::BufReader, path::Path};

use zip::ZipArchive;

use super::{
    files::{matches, write_hashed},
    layout::{remove_file, storage},
};
use crate::types::{AppError, ModelFile, ModelManifest, PortError, PortResult};

/// Unpacks every file of `manifest` from `archive` into `staging`; `on_block` gets the bytes written so far.
pub(super) async fn unpack(
    archive: &Path,
    staging: &Path,
    manifest: &ModelManifest,
    mut on_block: impl FnMut(u64),
) -> PortResult<()> {
    let file =
        File::open(archive).map_err(|error| storage("opening the archive", archive, &error))?;
    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|error| {
        PortError::new(AppError::ModelCorrupt {
            model_id: manifest.id.clone(),
        })
        .with_detail(format!("the archive cannot be read: {error}"))
    })?;
    let mut done = 0_u64;
    for member in manifest.files.iter() {
        let target = staging.join(member.name.as_str());
        let expected = member.bytes.get();
        let hasher = {
            let mut entry = zip
                .by_name(member.name.as_str())
                .map_err(|_| corrupt(manifest, member, "is not in the archive"))?;
            if entry.size() != expected {
                return Err(corrupt(manifest, member, "has another size in the archive"));
            }
            write_hashed(&mut entry, archive, &target, expected, |bytes| {
                on_block(done + bytes);
            })
            .await?
        };
        if !matches(hasher, member) {
            remove_file(&target)?;
            return Err(corrupt(manifest, member, "does not match its SHA-256"));
        }
        done += expected;
    }
    Ok(())
}

fn corrupt(manifest: &ModelManifest, member: &ModelFile, why: &str) -> PortError {
    PortError::new(AppError::ModelCorrupt {
        model_id: manifest.id.clone(),
    })
    .with_detail(format!("{} {why}", member.name))
}
