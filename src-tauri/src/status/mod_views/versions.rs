//! The installed versions of a mod, for the picker and the details view.

use crate::context::size_kb;
use crate::domain::profiles::ProfilesView;
use crate::library::{ArchiveLabel, Library, LibraryView, ModVersion};
use crate::status::views::VersionView;

/// Every installed version of a family, newest first, for the picker and the mod details.
pub(in crate::status) fn version_views(
    library: &Library,
    view: &LibraryView<'_>,
    profiles: &ProfilesView<'_>,
    family: &str,
) -> Vec<VersionView> {
    view.versions_of(family)
        .iter()
        .copied()
        .map(|version| version_view(library, profiles, version))
        .collect()
}

/// The original archive name of a version and, when `info.json` has no real name or version, the name and version a
/// provider read from it. The row and the version picker use the same rule, so the same version shows the same name in
/// both.
pub(in crate::status) fn archive_names<'a>(
    library: &'a Library,
    version: &ModVersion,
) -> (Option<&'a str>, Option<&'a ArchiveLabel>) {
    let archive_name = library.archive_name(&version.folder);
    // Most versions have a real name and version in `info.json`; the label is then only display text and is not
    // needed for every version of every mod.
    let needs_archive = version.info.name.is_empty()
        || version.info.name == version.folder
        || version.info.version.is_none();
    let label = needs_archive
        .then(|| library.archive_label_of(version))
        .flatten();
    (archive_name, label)
}

/// One installed version of a mod: its name and version as the archive gives them, and the profiles that use it.
pub(in crate::status) fn version_view(
    library: &Library,
    profiles: &ProfilesView<'_>,
    version: &ModVersion,
) -> VersionView {
    let (archive_name, label) = archive_names(library, version);
    version_view_with(library, profiles, version, archive_name, label)
}

/// `version_view` with an archive name and its label the caller already has.
fn version_view_with(
    library: &Library,
    profiles: &ProfilesView<'_>,
    version: &ModVersion,
    archive_name: Option<&str>,
    label: Option<&ArchiveLabel>,
) -> VersionView {
    VersionView {
        folder: version.folder.clone(),
        name: if version.info.name == version.folder {
            label
                .and_then(|label| label.name.clone())
                .unwrap_or_else(|| version.info.name.clone())
        } else {
            version.info.name.clone()
        },
        version: version
            .info
            .version
            .clone()
            .or_else(|| label.map(|label| label.version.clone())),
        files: version.files.len() as u32,
        size_kb: size_kb(version.size_bytes),
        archive: archive_name.map(str::to_string),
        archive_file: version
            .archive
            .as_ref()
            .and_then(|sha256| library.archive_file_of(sha256)),
        installed_at: version.installed_at,
        used_by: profiles.using_version(&version.folder).len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::source::ProviderId;
    use crate::library::{Archive, ModInfo};
    use std::collections::BTreeMap;

    #[test]
    fn a_label_a_provider_read_names_a_version_whose_info_has_no_name_of_its_own() {
        let mut library = Library::default();
        library.archives.insert(
            "sha".into(),
            Archive {
                original_name: "Example Pack 7 2.0 2026-01-01T00-00Z.zip".into(),
                file: "archive.zip".into(),
                named: Some(ArchiveLabel {
                    name: Some("Example Pack".into()),
                    version: "2.0".into(),
                }),
                source_files: BTreeMap::new(),
                source_parts: BTreeMap::new(),
                skipped: Vec::new(),
                skipped_total: 0,
            },
        );
        let mut version =
            crate::test_support::mod_version("folder", "example-host:7", "folder", None);
        version.archive = Some("sha".into());
        version
            .parsed_sources
            .insert(ProviderId::new("example-host"), "7".into());
        library
            .versions
            .insert(version.folder.clone(), version.clone());

        let (archive_name, label) = archive_names(&library, &version);
        assert_eq!(
            archive_name,
            Some("Example Pack 7 2.0 2026-01-01T00-00Z.zip")
        );
        assert_eq!(label.map(|label| label.version.as_str()), Some("2.0"));

        // A version with its own name and version in `info.json` does not need the label.
        let mut with_info = version.clone();
        with_info.info = ModInfo {
            name: "Own name".into(),
            version: Some("1.0".into()),
            ..ModInfo::default()
        };
        assert!(archive_names(&library, &with_info).1.is_none());
    }
}
