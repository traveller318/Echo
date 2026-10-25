/*!
 * SOURCE OF TRUTH KEYWORDS: tar.gz unpack, release archive Linux, soname symlink chain, executable bit, member digest, tar entries sequential scan, unpack_tar
 * WHAT:  `unpack_tar`: unpacks every file of a manifest from a `.tar.gz` release archive into the staging folder,
 *        checking each against its manifest size and SHA-256, and keeping the archive's executable bit.
 * WHY:   The Linux llama.cpp release is a tar.gz whose libraries are symlink chains (`libllama.so.0` ->
 *        `libllama.so.0.5.0`) under one top folder. A manifest names the file the loader asks for (the soname); the
 *        first pass reads every entry's link, so a name resolves to the real file whose bytes are written under the
 *        manifest name, and the second pass streams the real entries once. Where a file lands is decided by the
 *        manifest, never by an archive path, so a hostile archive cannot write outside the staging folder. A
 *        manifest name that resolves to nothing, to a file of another size, or to a digest mismatch is
 *        `ModelCorrupt`, as for zip. Mode bits come from the archive (the server binary must be executable); only
 *        the rwx bits of the owner and group/other read-execute are kept.
 * WHERE: archive.rs `unpack` for archives named `*.tar.gz` (Linux manifests).
 */

use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Cursor, Read},
    os::unix::fs::PermissionsExt,
    path::Path,
};

use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

use super::{
    files::{matches, write_hashed},
    layout::{remove_file, storage},
};
use crate::types::{AppError, ModelFile, ModelManifest, PortError, PortResult};

/// Symlink hops followed before a chain counts as a loop.
const MAX_HOPS: usize = 8;

/// Unpacks every file of `manifest` from the tar.gz `archive` into `staging`; `on_block` gets the bytes written so far.
pub(super) async fn unpack_tar(
    archive: &Path,
    staging: &Path,
    manifest: &ModelManifest,
    mut on_block: impl FnMut(u64),
) -> PortResult<()> {
    let links = read_links(archive, manifest)?;
    // The real archive path each manifest file is stored under.
    let mut wanted: HashMap<String, &ModelFile> = HashMap::new();
    for member in manifest.files.iter() {
        let real = resolve(&links, member.name.as_str())
            .ok_or_else(|| corrupt(manifest, member, "links in a loop in the archive"))?;
        if wanted.insert(real, member).is_some() {
            return Err(corrupt(
                manifest,
                member,
                "shares its archive file with another name",
            ));
        }
    }
    let found = read_members(archive, manifest, &wanted)?;
    let mut done = 0_u64;
    for (member, bytes, mode) in &found {
        let expected = member.bytes.get();
        let target = staging.join(member.name.as_str());
        let hasher = write_hashed(
            &mut Cursor::new(bytes),
            archive,
            &target,
            expected,
            |written| {
                on_block(done + written);
            },
        )
        .await?;
        if !matches(hasher, member) {
            remove_file(&target)?;
            return Err(corrupt(manifest, member, "does not match its SHA-256"));
        }
        std::fs::set_permissions(
            &target,
            std::fs::Permissions::from_mode(mode & 0o755 | 0o600),
        )
        .map_err(|error| storage("setting a model file's mode", &target, &error))?;
        done += expected;
    }
    if let Some(missing) = manifest
        .files
        .iter()
        .find(|member| !found.iter().any(|(found, _, _)| found.name == member.name))
    {
        return Err(corrupt(manifest, missing, "is not in the archive"));
    }
    Ok(())
}

/// One pass over the archive: the bytes and mode of every wanted entry, each checked for its manifest size.
///
/// A plain function with no await: the tar reader is not `Send`, so it must be gone before the caller awaits.
fn read_members<'m>(
    archive: &Path,
    manifest: &ModelManifest,
    wanted: &HashMap<String, &'m ModelFile>,
) -> PortResult<Vec<(&'m ModelFile, Vec<u8>, u32)>> {
    let mut tar = Archive::new(GzDecoder::new(open(archive)?));
    let mut found = Vec::new();
    for entry in tar
        .entries()
        .map_err(|error| unreadable(manifest, &error.to_string()))?
    {
        let mut entry = entry.map_err(|error| unreadable(manifest, &error.to_string()))?;
        if entry.header().entry_type() != EntryType::Regular {
            continue;
        }
        let Some(member) = entry_name(&entry).and_then(|name| wanted.get(&name).copied()) else {
            continue;
        };
        if entry.size() != member.bytes.get() {
            return Err(corrupt(manifest, member, "has another size in the archive"));
        }
        let mode = entry.header().mode().unwrap_or(0o644);
        let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| unreadable(manifest, &error.to_string()))?;
        found.push((member, bytes, mode));
    }
    Ok(found)
}

fn open(archive: &Path) -> PortResult<BufReader<File>> {
    File::open(archive)
        .map(BufReader::new)
        .map_err(|error| storage("opening the archive", archive, &error))
}

/// Every symlink in the archive, as `name -> target name` (both without the top folder).
fn read_links(archive: &Path, manifest: &ModelManifest) -> PortResult<HashMap<String, String>> {
    let mut tar = Archive::new(GzDecoder::new(open(archive)?));
    let mut links = HashMap::new();
    for entry in tar
        .entries()
        .map_err(|error| unreadable(manifest, &error.to_string()))?
    {
        let entry = entry.map_err(|error| unreadable(manifest, &error.to_string()))?;
        if entry.header().entry_type() != EntryType::Symlink {
            continue;
        }
        let (Some(name), Ok(Some(target))) = (entry_name(&entry), entry.link_name()) else {
            continue;
        };
        if let Some(target) = target.file_name().and_then(|name| name.to_str()) {
            links.insert(name, target.to_owned());
        }
    }
    Ok(links)
}

/// Follows `name` through `links` to the name of a real file; None for a loop.
fn resolve(links: &HashMap<String, String>, name: &str) -> Option<String> {
    let mut current = name;
    for _ in 0..MAX_HOPS {
        match links.get(current) {
            Some(next) => current = next,
            None => return Some(current.to_owned()),
        }
    }
    None
}

/// An entry's file name without its top folder; None for a nested path (the release keeps everything flat).
fn entry_name<R: std::io::Read>(entry: &tar::Entry<'_, R>) -> Option<String> {
    let path = entry.path().ok()?;
    let mut parts = path.components();
    parts.next()?;
    let name = parts.next()?.as_os_str().to_str()?.to_owned();
    parts.next().is_none().then_some(name)
}

fn unreadable(manifest: &ModelManifest, detail: &str) -> PortError {
    PortError::new(AppError::ModelCorrupt {
        model_id: manifest.id.clone(),
    })
    .with_detail(format!("the archive cannot be read: {detail}"))
}

fn corrupt(manifest: &ModelManifest, member: &ModelFile, why: &str) -> PortError {
    PortError::new(AppError::ModelCorrupt {
        model_id: manifest.id.clone(),
    })
    .with_detail(format!("{} {why}", member.name))
}

#[cfg(test)]
mod tests {
    use flate2::{Compression, write::GzEncoder};
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::types::{
        ByteCount, ModelId, ModelKind, Sha256Hex, StaticList, StaticStr, testing::TempDir,
    };

    fn links(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(from, to)| ((*from).to_owned(), (*to).to_owned()))
            .collect()
    }

    #[test]
    fn a_soname_resolves_through_its_chain_to_the_real_file() {
        let chain = links(&[
            ("libllama.so", "libllama.so.0"),
            ("libllama.so.0", "libllama.so.0.5.0"),
        ]);
        assert_eq!(
            resolve(&chain, "libllama.so.0").as_deref(),
            Some("libllama.so.0.5.0")
        );
        assert_eq!(
            resolve(&chain, "libllama.so").as_deref(),
            Some("libllama.so.0.5.0")
        );
        assert_eq!(
            resolve(&chain, "llama-server").as_deref(),
            Some("llama-server")
        );
    }

    #[test]
    fn a_link_loop_is_refused() {
        let looped = links(&[("a", "b"), ("b", "a")]);
        assert_eq!(resolve(&looped, "a"), None);
    }

    const SERVER: &[u8] = b"#!server binary";
    const LIBRARY: &[u8] = b"library bytes";

    /// A release-shaped tar.gz: one top folder, a executable, a library behind a two-link chain, and an unwanted file.
    fn release(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("tool.tar.gz");
        let mut builder = tar::Builder::new(GzEncoder::new(
            File::create(&path).unwrap(),
            Compression::default(),
        ));
        let file = |builder: &mut tar::Builder<_>, name: &str, mode: u32, bytes: &[u8]| {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(mode);
            header.set_cksum();
            builder
                .append_data(&mut header, format!("rel/{name}"), bytes)
                .unwrap();
        };
        let link = |builder: &mut tar::Builder<_>, name: &str, target: &str| {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            builder
                .append_link(&mut header, format!("rel/{name}"), target)
                .unwrap();
        };
        file(&mut builder, "llama-server", 0o755, SERVER);
        file(&mut builder, "libllama.so.0.5.0", 0o644, LIBRARY);
        link(&mut builder, "libllama.so.0", "libllama.so.0.5.0");
        link(&mut builder, "libllama.so", "libllama.so.0");
        file(&mut builder, "unwanted-tool", 0o755, b"not kept");
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    fn member(name: &str, bytes: &[u8]) -> ModelFile {
        ModelFile {
            name: StaticStr::from(name.to_owned()),
            url: StaticStr::new("https://example.invalid/tool.tar.gz"),
            sha256: Sha256Hex::from(
                Sha256::digest(bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            ),
            bytes: ByteCount::new(bytes.len() as u64),
        }
    }

    fn manifest(files: Vec<ModelFile>) -> ModelManifest {
        ModelManifest {
            id: ModelId::from_static("tool"),
            label: StaticStr::new("Tool"),
            kind: ModelKind::Runtime,
            license: StaticStr::new("MIT"),
            attribution: None,
            revision: StaticStr::new("1"),
            files: StaticList::from(files),
            archive: None,
            requires: StaticList::new(&[]),
            bundled: false,
        }
    }

    fn unpack(archive: &Path, staging: &Path, manifest: &ModelManifest) -> PortResult<()> {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(unpack_tar(archive, staging, manifest, |_| {}))
    }

    #[test]
    fn kept_files_land_under_their_manifest_names_with_the_archive_modes() {
        let dir = TempDir::new("tar-unpack");
        let archive = release(dir.path());
        let staging = dir.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let manifest = manifest(vec![
            member("llama-server", SERVER),
            member("libllama.so.0", LIBRARY),
        ]);
        unpack(&archive, &staging, &manifest).unwrap();
        assert_eq!(std::fs::read(staging.join("llama-server")).unwrap(), SERVER);
        assert_eq!(
            std::fs::read(staging.join("libllama.so.0")).unwrap(),
            LIBRARY
        );
        let mode = |name: &str| {
            std::fs::metadata(staging.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("llama-server"), 0o755);
        assert_eq!(mode("libllama.so.0"), 0o644);
        assert!(!staging.join("unwanted-tool").exists());
    }

    #[test]
    fn a_missing_or_different_file_is_a_corrupt_model() {
        let dir = TempDir::new("tar-corrupt");
        let archive = release(dir.path());
        let staging = dir.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        let corrupt = |files: Vec<ModelFile>| {
            unpack(&archive, &staging, &manifest(files))
                .err()
                .map(PortError::into_app_error)
        };
        let model_corrupt = Some(AppError::ModelCorrupt {
            model_id: ModelId::from_static("tool"),
        });
        assert_eq!(corrupt(vec![member("absent", b"x")]), model_corrupt);
        assert_eq!(
            corrupt(vec![member("llama-server", b"other bytes!!!")]),
            model_corrupt,
            "another size"
        );
        assert_eq!(
            corrupt(vec![member("llama-server", b"#!server binarX")]),
            model_corrupt,
            "same size, another digest"
        );
    }
}
