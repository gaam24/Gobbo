//! Selecting an installed version that a provider download brought, without a direct download.

use crate::context::Data;
use crate::domain::source::ProviderId;
use crate::error::AppError;
use crate::library::LibraryView;

/// Selects the installed version of `id` that matches the newest file of `provider`. The window calls it after a user
/// downloaded an update by hand, so that update also becomes the selected version in the active profile.
pub fn select_installed_update(
    root: &Data,
    game_id: &str,
    id: &str,
    provider: &str,
) -> Result<(), AppError> {
    let folder = super::with_family(root, game_id, id, |_, library, family| {
        let provider = ProviderId::new(provider);
        let source = family.sources.get(&provider);
        let has_part = source
            .and_then(|source| source.part_id.as_deref())
            .is_some();
        let latest = family.latest.get(&provider).cloned().or_else(|| {
            if has_part {
                return None;
            }

            source
                .and_then(|source| source.meta.as_ref())
                .and_then(|meta| meta.version.clone())
        });
        let latest_file = family.latest_file_ids.get(&provider).cloned();
        let view = LibraryView::new(library);
        view.installed_latest_folder(
            id,
            provider.as_str(),
            latest_file.as_deref(),
            latest.as_deref(),
        )
        .map(str::to_string)
        .ok_or_else(|| {
            AppError::user("source.no_version")
                .with("id", id)
                .with("provider", provider.as_str())
        })
    })?;
    crate::profiles::actions::set_mod_version(root, game_id, id, &folder)
}

#[cfg(test)]
mod tests {
    use crate::domain::source::ProviderId;
    use crate::library::{Archive, Library, LibraryView, ModVersion};
    use crate::test_support::mod_version;
    use std::collections::BTreeMap;

    fn version_with_file(folder: &str, version: &str) -> ModVersion {
        let mut record = mod_version(folder, "nexus:7", folder, Some(version));
        record.archive = Some(format!("sha-{folder}"));
        record
            .parsed_sources
            .insert(ProviderId::new("nexus"), "7".into());
        record
    }

    fn library_with_two_versions() -> Library {
        let mut library = Library::default();

        for (folder, version, file) in [("old", "1.0", "100"), ("new", "2.0", "200")] {
            let record = version_with_file(folder, version);
            let sha = record.archive.clone().expect("the test sets an archive");
            library.archives.insert(
                sha,
                Archive {
                    original_name: format!("{folder}.zip"),
                    file: format!("{folder}.zip"),
                    source_files: BTreeMap::from([(ProviderId::new("nexus"), file.to_string())]),
                    source_parts: BTreeMap::new(),
                    named: None,
                    skipped: Vec::new(),
                    skipped_total: 0,
                },
            );
            library.versions.insert(record.folder.clone(), record);
        }

        library
    }

    #[test]
    fn a_version_is_found_by_its_provider_file_first() {
        let library = library_with_two_versions();
        let view = LibraryView::new(&library);
        assert_eq!(
            view.installed_latest_folder("nexus:7", "nexus", Some("200"), None),
            Some("new")
        );
        assert_eq!(
            view.installed_latest_folder("nexus:7", "nexus", None, Some("1.0")),
            Some("old")
        );
        assert_eq!(
            view.installed_latest_folder("nexus:7", "nexus", None, Some("3.0")),
            None
        );
    }
}
