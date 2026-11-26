use crate::domain::library::view::LibraryView;
use crate::domain::library::{Archive, ArchiveLabel, Library, ModVersion};
use crate::domain::source::ProviderId;

fn version(
    folder: &str,
    family: &str,
    info_version: Option<&str>,
    parsed: Option<u32>,
) -> ModVersion {
    ModVersion {
        folder: folder.to_string(),
        family: family.to_string(),
        slot: String::new(),
        files: Vec::new().into(),
        size_bytes: 0,
        info: crate::domain::library::ModInfo {
            name: folder.to_string(),
            version: info_version.map(str::to_string),
            author: None,
            description: None,
        },
        edit: crate::domain::library::ModInfoEdit::default(),
        archive: None,
        parsed_sources: parsed.map_or_else(std::collections::BTreeMap::new, |id| {
            std::collections::BTreeMap::from([(ProviderId::new("nexus"), id.to_string())])
        }),
        parsed_parts: std::collections::BTreeMap::new(),
        installed_at: 0,
        fomod: Vec::new(),
        fomod_installer: false,
        integrity: None,
    }
}

fn library() -> Library {
    let mut library = Library::default();
    library
        .versions
        .insert("a".into(), version("a", "nexus:1", Some("1.9"), None));
    library
        .versions
        .insert("b".into(), version("b", "nexus:1", Some("1.10"), None));
    library
        .versions
        .insert("c".into(), version("c", "nexus:2", Some("2.0b"), Some(20)));
    library
        .versions
        .insert("d".into(), version("d", "nexus:2", Some("2.1"), Some(21)));
    let mut archived = version("e", "nexus:3", None, None);
    archived.archive = Some("sha".into());
    library.versions.insert("e".into(), archived);
    library.archives.insert(
        "sha".into(),
        Archive {
            original_name: "Example Mod 12345 1.2.0 2026-01-01T00-00Z x.zip".into(),
            file: "x.zip".into(),
            source_files: std::collections::BTreeMap::new(),
            source_parts: std::collections::BTreeMap::new(),
            skipped: Vec::new(),
            skipped_total: 0,
            named: None,
        },
    );
    library.families.insert(
        "nexus:1".into(),
        crate::domain::library::Family {
            id: "nexus:1".into(),
            name: "One".into(),
            ..crate::domain::library::Family::default()
        },
    );
    library.families.insert(
        "nexus:2".into(),
        crate::domain::library::Family {
            id: "nexus:2".into(),
            name: "Two".into(),
            sources: std::collections::BTreeMap::from([(
                ProviderId::new("nexus"),
                crate::domain::source::SourceMod {
                    mod_id: "2".into(),
                    part_id: None,
                    part_name: None,
                    part_category: None,
                    meta: None,
                },
            )]),
            ..crate::domain::library::Family::default()
        },
    );
    library
}

#[test]
fn versions_are_sorted_once_and_the_default_is_the_newest() {
    let library = library();
    let view = LibraryView::new(&library);
    let versions: Vec<&str> = view
        .versions_of("nexus:1")
        .iter()
        .map(|version| version.folder.as_str())
        .collect();
    assert_eq!(versions, ["b", "a"]);
    assert_eq!(
        view.default_version("nexus:1").map(|v| v.folder.as_str()),
        Some("b")
    );
    assert_eq!(
        view.default_version("nexus:3").map(|v| v.folder.as_str()),
        Some("e")
    );
    assert!(view.versions_of("nexus:none").is_empty());
}

#[test]
fn a_family_order_matches_the_old_filter_and_sort() {
    let mut library = library();
    // The same version and install time as `d`: the folder name decides, as the old comparator did.
    library
        .versions
        .insert("f".into(), version("f", "nexus:2", Some("2.1"), None));
    let view = LibraryView::new(&library);

    let actual: Vec<&str> = view
        .versions_of("nexus:2")
        .iter()
        .map(|version| version.folder.as_str())
        .collect();
    // The version, then the install time, then the folder name decide; the order is the one the old
    // filter-and-sort produced.
    assert_eq!(actual, ["d", "f", "c"]);
}

#[test]
fn a_version_is_installed_by_its_info_version_only_in_its_own_family() {
    let library = library();
    let view = LibraryView::new(&library);
    let installed = |family: &str, wanted: &str| {
        view.installed_latest_folder(family, "nexus", None, Some(wanted))
            .is_some()
    };
    assert!(installed("nexus:1", "1.9"));
    assert!(installed("nexus:1", "1.10"));
    assert!(!installed("nexus:1", "2.0"));
    // The versions of `nexus:2` are not versions of `nexus:1`.
    assert!(!installed("nexus:1", "2.1"));
    assert!(installed("nexus:2", "2.1"));
}

#[test]
fn a_version_is_matched_by_the_label_its_provider_read_from_the_archive() {
    let mut library = library();
    // The file name still says 1.2.0, but only the label the provider stored counts.
    library.archives.get_mut("sha").unwrap().named = Some(ArchiveLabel {
        name: Some("Example Pack".into()),
        version: "3.1".into(),
    });
    let view = LibraryView::new(&library);
    assert!(
        view.installed_latest_folder("nexus:3", "nexus", None, Some("3.1"))
            .is_some()
    );
    assert!(
        view.installed_latest_folder("nexus:3", "nexus", None, Some("1.2.0"))
            .is_none()
    );
    assert_eq!(
        library.version_string(library.versions.get("e").unwrap()),
        Some("3.1".into())
    );
}

#[test]
fn an_exact_provider_file_finds_only_the_version_from_that_archive() {
    // Two parts of one page, each from its own archive and file id. The exact file must resolve to its own version.
    let provider = ProviderId::new("test-provider");
    let mut library = Library::default();
    let mut first_part = version("first-part", "test:7#1", Some("1.0"), Some(7));
    first_part.parsed_sources = std::collections::BTreeMap::from([(provider.clone(), "7".into())]);
    first_part.archive = Some("first-sha".into());
    let mut second_part = version("second-part", "test:7#2", Some("2.0"), Some(7));
    second_part.parsed_sources = std::collections::BTreeMap::from([(provider.clone(), "7".into())]);
    second_part.archive = Some("second-sha".into());
    library.versions.insert("first-part".into(), first_part);
    library.versions.insert("second-part".into(), second_part);

    for (sha, file_id) in [("first-sha", "101"), ("second-sha", "202")] {
        library.archives.insert(
            sha.into(),
            Archive {
                original_name: format!("{file_id}.zip"),
                file: format!("{file_id}.zip"),
                named: None,
                source_files: std::collections::BTreeMap::from([(
                    provider.clone(),
                    file_id.to_string(),
                )]),
                source_parts: std::collections::BTreeMap::new(),
                skipped: Vec::new(),
                skipped_total: 0,
            },
        );
    }

    for (id, name) in [("test:7#1", "First part"), ("test:7#2", "Second part")] {
        library.families.insert(
            id.into(),
            crate::domain::library::Family {
                id: id.into(),
                name: name.into(),
                sources: std::collections::BTreeMap::from([(
                    provider.clone(),
                    crate::domain::source::SourceMod {
                        mod_id: "7".into(),
                        part_id: None,
                        part_name: None,
                        part_category: None,
                        meta: None,
                    },
                )]),
                ..crate::domain::library::Family::default()
            },
        );
    }

    let view = LibraryView::new(&library);
    assert_eq!(
        view.version_of_file(&provider, "7", "101")
            .map(|version| version.folder.as_str()),
        Some("first-part")
    );
    assert_eq!(
        view.version_of_file(&provider, "7", "202")
            .map(|version| version.folder.as_str()),
        Some("second-part")
    );
    assert!(view.version_of_file(&provider, "7", "303").is_none());
}
