//! Tests of the row views: the saved check and the description preview.

use super::rows::{LibraryContext, mod_view, update_report};
use super::versions::archive_names;
use crate::domain::library::{Archive, Family, Library, LibraryView, ModVersion};
use crate::domain::profiles::ProfilesView;
use crate::domain::profiles::entry::ProfileMod;
use crate::domain::source::{ProviderId, SourceMeta, SourceMod};
use crate::domain::version::{IgnoredVersion, UpdateState};
use crate::library::relations::{SourceLookup, UnmetRequirements};
use crate::status::PlanOutcome;
use crate::status::views::{UpdateReport, description_preview};
use std::collections::BTreeMap;

#[test]
fn a_long_description_is_shortened_at_a_character_boundary() {
    let text = "é".repeat(600);
    let preview = description_preview(&text);
    assert_eq!(preview.chars().count(), 513);
    assert!(preview.ends_with('…'));
    assert_eq!(description_preview("short"), "short");
}

fn installed(folder: &str, info_version: &str) -> ModVersion {
    crate::test_support::mod_version(folder, "nexus:7", folder, Some(info_version))
}

/// A Nexus mod whose page was read last with the newest version `2.0`, at `checked_at`.
fn checked_family(checked_at: Option<u32>) -> Family {
    let source = SourceMod {
        mod_id: "7".into(),
        part_id: None,
        part_name: None,
        part_category: None,
        meta: Some(SourceMeta {
            version: Some("2.0".into()),
            checked_at,
            ..SourceMeta::default()
        }),
    };
    Family {
        id: "nexus:7".into(),
        name: "Seven".into(),
        sources: BTreeMap::from([(ProviderId::new("nexus"), source)]),
        ..Family::default()
    }
}

fn library_with(versions: Vec<ModVersion>, family: Family) -> Library {
    let mut library = Library::default();

    for version in versions {
        library.versions.insert(version.folder.clone(), version);
    }

    library.families.insert(family.id.clone(), family);
    library
}

fn saved_report(library: &Library, folder: &str) -> UpdateReport {
    let view = LibraryView::new(library);
    update_report(
        library.families.get("nexus:7"),
        library.versions.get(folder),
        &view,
    )
    .expect("a check was saved")
}

#[test]
fn a_saved_check_says_whether_the_selected_version_is_behind() {
    let library = library_with(
        vec![installed("old", "1.0")],
        checked_family(Some(1_730_000_000)),
    );
    let report = saved_report(&library, "old");
    assert_eq!(report.id, "nexus:7");
    assert_eq!(report.provider, "nexus");
    assert_eq!(report.mod_id, "7");
    assert_eq!(report.current.as_deref(), Some("1.0"));
    assert_eq!(report.latest.as_deref(), Some("2.0"));
    assert_eq!(report.latest_file_id, None);
    assert_eq!(report.state, UpdateState::Newer);
    assert!(!report.latest_installed);
    assert!(report.error.is_none());
}

#[test]
fn a_saved_check_carries_the_exact_file_a_manual_update_has_to_download() {
    let mut family = checked_family(Some(1_730_000_000));
    family
        .latest_file_ids
        .insert(ProviderId::new("nexus"), "200".into());
    let library = library_with(vec![installed("old", "1.0")], family);
    let report = saved_report(&library, "old");
    assert_eq!(report.latest_file_id.as_deref(), Some("200"));
}

#[test]
fn the_saved_result_follows_the_version_the_profile_selects() {
    let library = library_with(
        vec![installed("old", "1.0"), installed("new", "2.0")],
        checked_family(Some(1_730_000_000)),
    );
    // The newest version is installed but not selected: the update is still there, and it is already installed.
    let behind = saved_report(&library, "old");
    assert_eq!(behind.state, UpdateState::Newer);
    assert!(behind.latest_installed);
    // Once the newest version is selected, nothing is behind.
    let newest = saved_report(&library, "new");
    assert_eq!(newest.current.as_deref(), Some("2.0"));
    assert_eq!(newest.state, UpdateState::Current);
}

#[test]
fn the_latest_of_one_part_wins_over_the_page_version() {
    let mut family = checked_family(Some(1_730_000_000));
    family.latest.insert(ProviderId::new("nexus"), "1.5".into());
    let library = library_with(vec![installed("old", "1.0")], family);
    let report = saved_report(&library, "old");
    assert_eq!(report.latest.as_deref(), Some("1.5"));
}

#[test]
fn a_new_file_id_of_the_part_is_an_update_even_when_the_version_string_sorts_lower() {
    let nexus = ProviderId::new("nexus");
    let mut family = checked_family(Some(1_730_000_000));
    family.latest.insert(nexus.clone(), "1.1.3a".into());
    family.latest_file_ids.insert(nexus.clone(), "20".into());
    let mut old = installed("old", "1.1.3");
    old.archive = Some("old-sha".into());
    let mut new = installed("new", "1.1.3a");
    new.archive = Some("new-sha".into());
    let mut library = library_with(vec![old, new], family);
    library.archives.insert(
        "old-sha".into(),
        Archive {
            original_name: "Mod 1.1.3".into(),
            file: "old.zip".into(),
            source_files: BTreeMap::from([(nexus.clone(), "10".into())]),
            source_parts: BTreeMap::from([(nexus.clone(), "50".into())]),
            named: None,
            skipped: Vec::new(),
            skipped_total: 0,
        },
    );
    library.archives.insert(
        "new-sha".into(),
        Archive {
            original_name: "Mod 1.1.3a".into(),
            file: "new.zip".into(),
            source_files: BTreeMap::from([(nexus.clone(), "20".into())]),
            source_parts: BTreeMap::from([(nexus, "50".into())]),
            named: None,
            skipped: Vec::new(),
            skipped_total: 0,
        },
    );
    let behind = saved_report(&library, "old");
    assert_eq!(behind.latest.as_deref(), Some("1.1.3a"));
    assert_eq!(behind.state, UpdateState::Newer);
    assert!(behind.latest_installed);
    let newest = saved_report(&library, "new");
    assert_eq!(newest.state, UpdateState::Current);
}

#[test]
fn a_version_that_cannot_be_ordered_is_different_not_newer() {
    let mut family = checked_family(Some(1_730_000_000));
    family
        .latest
        .insert(ProviderId::new("nexus"), "1.1.3a".into());
    let library = library_with(vec![installed("old", "1.1.3")], family);
    let report = saved_report(&library, "old");
    assert_eq!(report.state, UpdateState::Different);
    assert_eq!(report.latest.as_deref(), Some("1.1.3a"));
}

#[test]
fn an_ignored_version_stays_out_of_the_change_state() {
    let nexus = ProviderId::new("nexus");
    let mut family = checked_family(Some(1_730_000_000));
    family.latest.insert(nexus.clone(), "1.1.3a".into());
    family.ignored_versions.insert(
        nexus,
        IgnoredVersion {
            version: "1.1.3a".into(),
            file_id: None,
        },
    );
    let library = library_with(vec![installed("old", "1.1.3")], family);
    let report = saved_report(&library, "old");
    assert_eq!(report.state, UpdateState::Ignored);
}

#[test]
fn a_page_without_a_check_time_is_not_a_check() {
    let library = library_with(vec![installed("old", "1.0")], checked_family(None));
    let view = LibraryView::new(&library);
    assert!(
        update_report(
            library.families.get("nexus:7"),
            library.versions.get("old"),
            &view,
        )
        .is_none()
    );
}

#[test]
fn a_row_takes_its_page_from_the_same_source_as_its_preview() {
    let mut family = Family {
        id: "nexus:7".into(),
        name: "Family".into(),
        ..Family::default()
    };
    // The first provider has no page data; the second one has the name the row has to show.
    family.sources.insert(
        ProviderId::new("nexus"),
        SourceMod {
            mod_id: "7".into(),
            part_id: None,
            part_name: None,
            part_category: None,
            meta: None,
        },
    );
    family.sources.insert(
        ProviderId::new("thunderstore"),
        SourceMod {
            mod_id: "team-mod".into(),
            part_id: Some("part".into()),
            part_name: Some("Part".into()),
            part_category: Some("main".into()),
            meta: Some(SourceMeta {
                name: Some("Page Name".into()),
                ..SourceMeta::default()
            }),
        },
    );
    let library = library_with(Vec::new(), family);
    let entry = ProfileMod {
        id: "nexus:7".into(),
        folder: "folder".into(),
        slot: String::new(),
        enabled: true,
        priority: None,
    };
    let view = LibraryView::new(&library);
    let profiles = crate::domain::profiles::Profiles::with_default(Vec::new());
    let profiles_view = ProfilesView::new(&profiles);
    let lookup = SourceLookup::new(&view, std::slice::from_ref(&entry));
    let unmet = UnmetRequirements::of(&library, &lookup, std::slice::from_ref(&entry));
    let context = LibraryContext {
        library: &library,
        view: &view,
        lookup: &lookup,
        orderable: &[],
    };
    let row = mod_view(
        0,
        &entry,
        &crate::test_support::games::slot(),
        &context,
        &profiles_view,
        &PlanOutcome::default(),
        &unmet,
    );

    let preview = row.source.as_ref().expect("the row keeps a preview");
    assert_eq!(preview.provider, "thunderstore");
    assert_eq!(row.page_id.as_deref(), Some("thunderstore:team-mod"));
    assert_eq!(row.page_name, preview.name);
    assert_eq!(row.page_name.as_deref(), Some("Page Name"));
}

#[test]
fn the_archive_label_is_used_only_when_info_json_has_no_real_name_or_version() {
    let mut library = Library::default();
    library.archives.insert(
        "sha".into(),
        Archive {
            original_name: "Example Cleanup Mod 7001 1.2.0 2026-10-23T17-21Z example-uploader.zip"
                .into(),
            file: "archive.zip".into(),
            source_files: BTreeMap::new(),
            source_parts: BTreeMap::new(),
            named: Some(crate::domain::library::ArchiveLabel {
                name: Some("Example Cleanup Mod".into()),
                version: "1.2.0".into(),
            }),
            skipped: Vec::new(),
            skipped_total: 0,
        },
    );
    let mut version = installed("folder", "");
    version.archive = Some("sha".into());
    library
        .versions
        .insert(version.folder.clone(), version.clone());

    let archive = "Example Cleanup Mod 7001 1.2.0 2026-10-23T17-21Z example-uploader.zip";
    let (archive_name, parsed) = archive_names(&library, &version);
    assert_eq!(archive_name, Some(archive));
    assert!(parsed.is_some());

    let mut named = version.clone();
    named.info.name = "Own Name".into();
    named.info.version = Some("1.0".into());
    let (archive_name, parsed) = archive_names(&library, &named);
    assert_eq!(archive_name, Some(archive));
    assert!(parsed.is_none());
}
