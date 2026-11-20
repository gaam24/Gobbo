//! A bundle is a folder inside the collection archive, named by `source.fileExpression` under `bundled/`. Gobbo turns
//! it into a normal mod archive: a single archive inside the folder is used as it is; any other tree is zipped.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::domain::collection::CollectionItem;
use crate::domain::naming::normalized_relative_path;
use crate::error::{AppError, io_at};
use crate::infra::archive::Archive;

/// The largest bundle tree Gobbo writes. A bundle is a mod, not a game.
const MAX_BUNDLE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Turns the bundle of `item` into an archive in `<cache>/bundles/` and returns its path.
pub fn materialize(
    archive: &Path,
    item: &CollectionItem,
    cache: &Path,
) -> Result<PathBuf, AppError> {
    let relative = item
        .bundle_path()
        .ok_or_else(|| AppError::user("collection.bundle_missing").with("name", &item.name))?;
    let reader = Archive::open(archive).map_err(AppError::from)?;
    let prefix = format!("bundled/{relative}");
    let file = reader
        .entries()
        .iter()
        .enumerate()
        .find(|(_, entry)| !entry.is_dir && entry.name == prefix)
        .map(|(index, _)| index);
    let members: Vec<(usize, &str)> = reader
        .entries()
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            if entry.is_dir {
                return None;
            }

            let rest = entry.name.strip_prefix(&prefix)?.strip_prefix('/')?;
            Some((index, rest))
        })
        .collect();
    let bundle_dir = cache.join("bundles");
    fs::create_dir_all(&bundle_dir).map_err(io_at(&bundle_dir))?;
    let stem = crate::domain::naming::safe_limited_file_name(&item.key);

    if let Some(index) = file {
        let name = Path::new(&reader.entries()[index].name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("bundle");
        let target = bundle_dir.join(format!(
            "{stem}-{}",
            crate::domain::naming::safe_limited_file_name(name)
        ));
        extract(&reader, index, &target, MAX_BUNDLE_BYTES)?;
        return Ok(target);
    }

    if members.is_empty() {
        return Err(AppError::user("collection.bundle_missing").with("name", &item.name));
    }
    // A folder that holds exactly one archive is a wrapped download; importing that archive is what the curator meant.
    if members.len() == 1 && crate::infra::archive::is_archive(members[0].1) {
        let (index, name) = members[0];
        let target = bundle_dir.join(format!(
            "{stem}-{}",
            crate::domain::naming::safe_limited_file_name(name)
        ));
        extract(&reader, index, &target, MAX_BUNDLE_BYTES)?;
        return Ok(target);
    }

    let staging = bundle_dir.join(format!("{stem}-staging"));

    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(io_at(&staging))?;
    }

    fs::create_dir_all(&staging).map_err(io_at(&staging))?;
    let mut paths: BTreeMap<usize, PathBuf> = BTreeMap::new();
    let mut total = 0u64;

    for (index, relative) in &members {
        let Some(relative) = normalized_relative_path(relative) else {
            let _ = fs::remove_dir_all(&staging);
            return Err(AppError::user("collection.bundle_path").with("name", &item.name));
        };
        total = total.saturating_add(reader.entries()[*index].size);

        if total > MAX_BUNDLE_BYTES {
            let _ = fs::remove_dir_all(&staging);
            return Err(AppError::user("collection.bundle_too_large").with("name", &item.name));
        }

        let target = staging.join(&relative);

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_at(parent))?;
        }

        paths.insert(*index, target);
    }

    let wanted: std::collections::BTreeSet<usize> = paths.keys().copied().collect();
    reader.visit::<AppError>(&wanted, |index, data| {
        let Some(target) = paths.get(&index) else {
            return Ok(());
        };
        let mut file = fs::File::create(target).map_err(io_at(target))?;
        std::io::copy(&mut data.take(MAX_BUNDLE_BYTES), &mut file).map_err(io_at(target))?;
        Ok(())
    })?;
    let target = bundle_dir.join(format!("{stem}.zip"));
    zip_tree(&staging, &target)?;
    let _ = fs::remove_dir_all(&staging);
    Ok(target)
}

fn extract(reader: &Archive, index: usize, target: &Path, limit: u64) -> Result<(), AppError> {
    let wanted = std::collections::BTreeSet::from([index]);
    reader.visit::<AppError>(&wanted, |_, data| {
        let mut file = fs::File::create(target).map_err(io_at(target))?;
        std::io::copy(&mut data.take(limit.saturating_add(1)), &mut file).map_err(io_at(target))?;
        Ok(())
    })?;
    Ok(())
}

fn zip_tree(source: &Path, target: &Path) -> Result<(), AppError> {
    let file = fs::File::create(target).map_err(io_at(target))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut entries = Vec::new();
    collect(source, source, &mut entries)?;
    entries.sort();

    for (path, name) in entries {
        writer
            .start_file(name, options)
            .map_err(|error| AppError::user("collection.bundle_zip").with("detail", error))?;
        let mut file = fs::File::open(&path).map_err(io_at(&path))?;
        std::io::copy(&mut file, &mut writer).map_err(io_at(&path))?;
    }

    writer
        .finish()
        .map_err(|error| AppError::user("collection.bundle_zip").with("detail", error))?;
    Ok(())
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String)>) -> Result<(), AppError> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(io_at(dir))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();

        if path.is_dir() {
            collect(root, &path, out)?;
            continue;
        }

        let relative = path.strip_prefix(root).map_err(AppError::unexpected)?;
        let name = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        out.push((path, name));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::collection::{CollectionItem, CollectionSource, CollectionUpdatePolicy};
    use std::collections::BTreeMap;

    fn item() -> CollectionItem {
        CollectionItem {
            key: "bundle:test".into(),
            name: "Bundle".into(),
            version: None,
            author: None,
            category: None,
            summary: String::new(),
            picture_url: None,
            optional: false,
            phase: 0,
            source: CollectionSource::Bundle {
                path: "Bundled - Test".into(),
                tag: Some("test".into()),
            },
            file_name: None,
            size_kb: 0,
            md5: None,
            update_policy: CollectionUpdatePolicy::Exact,
            choices: Vec::new(),
            patches: BTreeMap::new(),
            instructions: String::new(),
            adult: false,
        }
    }

    #[test]
    fn a_bundle_folder_becomes_a_zip() {
        let dir = crate::test_support::scratch!("collection-bundle");
        let archive = dir.join("collection.7z");
        let mut writer = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
        let nested = dir.join("file.txt");
        fs::write(&nested, b"hello").unwrap();
        let entry = sevenz_rust2::ArchiveEntry::from_path(
            &nested,
            "bundled/Bundled - Test/sub/file.txt".to_string(),
        );
        writer
            .push_archive_entry(entry, Some(fs::File::open(&nested).unwrap()))
            .unwrap();
        writer.finish().unwrap();
        let target = materialize(&archive, &item(), &dir).unwrap();
        assert!(target.is_file());
        let zip = Archive::open(&target).unwrap();
        assert!(
            zip.entries()
                .iter()
                .any(|entry| entry.name == "sub/file.txt")
        );
        let _ = fs::remove_dir_all(dir);
    }
}
