//! The curator's binary patches: `patches/<mod name>/<file>.diff` in the collection archive, applied to an imported
//! mod after its files are in the library. The CRC32 of the source file is the guard the collection stores; a mismatch
//! skips the patch instead of writing a wrong file.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use crate::domain::collection::{CollectionIssue, CollectionIssueSeverity};
use crate::error::{AppError, Message, io_at};
use crate::infra::archive::Archive;
use crate::infra::store;

/// The largest patch file Gobbo applies. Real diffs are tiny next to the file they patch.
const MAX_PATCH_BYTES: u64 = 64 * 1024 * 1024;
/// The largest source file a patch may target. A larger file is more likely a game asset than a patched mod file.
const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;

/// Applies every patch of `patches` to `folder`. Issues for a missing patch, a wrong CRC or a damaged diff are added to
/// `issues`; a patch failure never removes the imported mod.
pub fn apply(
    archive: &Path,
    mod_name: &str,
    folder: &Path,
    patches: &BTreeMap<String, String>,
    issues: &mut Vec<CollectionIssue>,
) -> Result<usize, AppError> {
    if patches.is_empty() {
        return Ok(0);
    }
    let reader = Archive::open(archive).map_err(AppError::from)?;
    let prefix = format!("patches/{}/", mod_name.trim_matches('/'));
    let patch_entry = |file: &str| {
        let wanted = format!("{prefix}{file}.diff");
        reader
            .entries()
            .iter()
            .position(|entry| !entry.is_dir && entry.name == wanted)
    };
    let mut applied = 0;
    for (file, expected_crc) in patches {
        let Some(entry) = patch_entry(file) else {
            issues.push(issue(
                "collection.patch_missing",
                file,
                "the collection archive has no diff for this file",
            ));
            continue;
        };
        let source = folder.join(file);
        let metadata = match fs::metadata(&source) {
            Ok(metadata) if metadata.is_file() => metadata,
            _ => {
                issues.push(issue(
                    "collection.patch_source_missing",
                    file,
                    "the imported mod has no file the patch targets",
                ));
                continue;
            }
        };
        if metadata.len() > MAX_SOURCE_BYTES {
            issues.push(issue(
                "collection.patch_source_too_large",
                file,
                "the source file is too large to patch",
            ));
            continue;
        }
        let actual_crc = crc32(&source).map_err(io_at(&source))?;
        if !actual_crc.eq_ignore_ascii_case(expected_crc) {
            issues.push(issue(
                "collection.patch_crc_mismatch",
                file,
                "the installed file is not the one the patch was made from",
            ));
            continue;
        }
        let patch = reader
            .read_bytes_limited(entry, MAX_PATCH_BYTES)
            .map_err(AppError::from)?;
        let mut old = Vec::new();
        fs::File::open(&source)
            .and_then(|mut file| file.read_to_end(&mut old))
            .map_err(io_at(&source))?;
        let mut new = Vec::new();
        let mut cursor = Cursor::new(&patch);
        bsdiff::patch(&old, &mut cursor, &mut new)
            .map_err(|error| AppError::user("collection.patch_failed").with("detail", error))?;
        let temp = source.with_extension(format!("patch-{}", store::unique_suffix()));
        fs::write(&temp, &new).map_err(io_at(&temp))?;
        store::replace_file_clean(&temp, &source)?;
        applied += 1;
    }
    Ok(applied)
}

fn crc32(path: &Path) -> std::io::Result<String> {
    let mut hasher = crc32fast::Hasher::new();
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:08X}", hasher.finalize()))
}

fn issue(code: &str, path: &str, detail: &str) -> CollectionIssue {
    CollectionIssue {
        severity: CollectionIssueSeverity::Warning,
        item: None,
        message: Message::new(code).with("path", path).with("detail", detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crc_mismatch_is_reported_and_the_file_stays() {
        let dir = crate::test_support::scratch!("collection-patch");
        let folder = dir.join("mod");
        fs::create_dir_all(&folder).unwrap();
        let file = folder.join("x.txt");
        fs::write(&file, b"new").unwrap();
        let archive = dir.join("collection.7z");
        let mut writer = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
        let patch = dir.join("x.diff");
        fs::write(&patch, b"not a diff").unwrap();
        let entry =
            sevenz_rust2::ArchiveEntry::from_path(&patch, "patches/Example/x.txt.diff".to_string());
        writer
            .push_archive_entry(entry, Some(fs::File::open(&patch).unwrap()))
            .unwrap();
        writer.finish().unwrap();
        let patches = BTreeMap::from([("x.txt".to_string(), "00000000".to_string())]);
        let mut issues = Vec::new();
        let applied = apply(&archive, "Example", &folder, &patches, &mut issues).unwrap();
        assert_eq!(applied, 0);
        assert_eq!(issues[0].message.code, "collection.patch_crc_mismatch");
        assert_eq!(fs::read(&file).unwrap(), b"new");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_patch_map_does_nothing() {
        let mut issues = Vec::new();
        assert_eq!(
            apply(
                Path::new("missing.7z"),
                "Example",
                Path::new("."),
                &BTreeMap::new(),
                &mut issues
            )
            .unwrap(),
            0
        );
    }
}
