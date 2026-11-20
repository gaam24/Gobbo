//! Archives: the digest that finds an archive again, and the ones no version uses any more.

use super::*;
use crate::domain::source::ProviderId;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn archive_digest_is_found_again_from_its_versions() {
    let base = scratch("digest");
    let mut library = load_mods(&base, &["modA"]);
    link_archive_a(&mut library);

    let view = crate::domain::library::LibraryView::new(&library);
    let folders = |sha256| {
        view.versions_from_archive(sha256)
            .iter()
            .map(|version| version.folder.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(folders("sha-a"), ["modA"]);
    assert!(folders("sha-b").is_empty());
}

#[test]
fn archives_that_no_version_uses_are_forgotten() {
    let base = scratch("forget");
    let mut library = load_mods(&base, &["modA"]);
    library.versions.get_mut("modA").unwrap().archive = Some("sha-a".to_string());

    for sha256 in ["sha-a", "sha-old"] {
        library.archives.insert(
            sha256.to_string(),
            Archive {
                original_name: format!("{sha256}.zip"),
                file: format!("{sha256}.zip"),
                source_files: std::collections::BTreeMap::new(),
                source_parts: std::collections::BTreeMap::new(),
                named: None,
                skipped: Vec::new(),
                skipped_total: 0,
            },
        );
    }

    let kept = library.forget_unused_archives();
    assert_eq!(kept, BTreeSet::from(["sha-a.zip".to_string()]));
    assert_eq!(library.archives.keys().collect::<Vec<_>>(), ["sha-a"]);
}

#[test]
fn a_label_a_provider_reads_is_stored_once_and_matches_its_version() {
    let base = scratch("labels-provider");
    let mut library = load_mods(&base, &["modA"]);
    link_archive_a(&mut library);
    let family = library.versions["modA"].family.clone();
    library.versions.get_mut("modA").unwrap().parsed_sources =
        BTreeMap::from([(ProviderId::new("example-host"), "7".to_string())]);

    // Only the provider that named the version reads the file name; the label it gives is the one the library keeps.
    let read = |provider_ids: &[&str], _original_name: &str| {
        provider_ids
            .contains(&"example-host")
            .then(|| ArchiveLabel {
                name: Some("Example Pack".into()),
                version: "3.1".into(),
            })
    };
    assert!(library.name_archives(&read));
    assert!(
        !library.name_archives(&read),
        "a labelled archive is not read again"
    );

    let view = LibraryView::new(&library);
    assert!(
        view.installed_latest_folder(&family, "example-host", None, Some("3.1"))
            .is_some()
    );
    assert_eq!(
        library.version_string(&library.versions["modA"]).as_deref(),
        Some("3.1")
    );
}

#[test]
fn an_archive_no_provider_reads_keeps_no_label() {
    let base = scratch("labels-none");
    let mut library = load_mods(&base, &["modA"]);
    link_archive_a(&mut library);
    let nothing = |_: &[&str], _: &str| None;

    assert!(!library.name_archives(&nothing));
    assert!(library.archives["sha-a"].named.is_none());
}
