//! Matching an archive to a Nexus file: the exact name first, then the Nexus-shaped stem.

use super::ModFile;

/// The newest file of `files` whose name is the name of one of the archives, with that archive's digest. The names are
/// compared exactly first and then by their Nexus-shaped stems, because v3 gives the version's display name while an
/// archive carries the id, the version and a timestamp after it.
pub fn newest_match(files: &[ModFile], archives: &[(String, String)]) -> Option<(String, ModFile)> {
    // Normalize each name once. The nested loop used to allocate two strings for every file/archive pair.
    let file_names: Vec<String> = files
        .iter()
        .map(|file| normalized_stem(&file.file_name))
        .collect();
    let archive_names: Vec<String> = archives
        .iter()
        .map(|(_, name)| normalized_stem(name))
        .collect();
    let mut best: Option<((u8, usize, u32), usize, String)> = None;

    for (at, (file, file_norm)) in files.iter().zip(&file_names).enumerate() {
        for ((digest, name), archive_norm) in archives.iter().zip(&archive_names) {
            let Some((rank, length)) = archive_rank(&file.file_name, name, file_norm, archive_norm)
            else {
                continue;
            };
            let key = (rank, length, file.file_id);

            if best.as_ref().is_none_or(|(known, _, _)| key > *known) {
                best = Some((key, at, digest.clone()));
            }
        }
    }

    best.and_then(|(_, at, digest)| files.get(at).cloned().map(|file| (digest, file)))
}

/// The newest file of `files` that has the name of the archive, for a pack that names an archive but no file id.
pub fn find_file_id(files: &[ModFile], archive_name: &str) -> Option<u32> {
    let archive_norm = normalized_stem(archive_name);
    files
        .iter()
        .filter_map(|file| {
            let file_norm = normalized_stem(&file.file_name);
            let (rank, length) =
                archive_rank(&file.file_name, archive_name, &file_norm, &archive_norm)?;
            Some(((rank, length, file.file_id), file.file_id))
        })
        .max()
        .map(|(_, file_id)| file_id)
}

/// How well a v3 version name names an archive, and how specific it is. The normalized stems are passed in because
/// the caller compares one file with many archives.
fn archive_rank(
    file_name: &str,
    archive_name: &str,
    file_norm: &str,
    archive_norm: &str,
) -> Option<(u8, usize)> {
    if file_norm.is_empty() || archive_norm.is_empty() {
        return None;
    }

    if file_name.eq_ignore_ascii_case(archive_name) {
        return Some((2, file_norm.len()));
    }

    if archive_norm == file_norm {
        return Some((1, file_norm.len()));
    }

    archive_norm
        .starts_with(file_norm)
        .then_some((0, file_norm.len()))
}

/// A name without its archive extension, folded the same way the app folds every path key. A Nexus archive carries the
/// mod id, the version and a timestamp after the display name, so the version name is a prefix of it.
fn normalized_stem(name: &str) -> String {
    crate::domain::pathkey::fold_case(crate::domain::archive::strip_extension(name.trim()))
}
