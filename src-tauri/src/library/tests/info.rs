//! What the user types and what Nexus names keep over a rescan: the page data, the typed id and the names.

use super::*;
use crate::context::library::INDEX_FILE;
use crate::domain::source::{ProviderId, SourceMeta, SourceMod};

#[test]
fn rescan_keeps_the_nexus_page_data_and_the_typed_id() {
    let base = scratch("rescan-nexus");
    let library_dir = base.join("library");
    write(&library_dir.join("modA/content/x.ws"), "a");
    let mut library = Library::load(&base.join(INDEX_FILE), &library_dir, &game()).unwrap();
    let family = library.versions["modA"].family.clone();
    let record = library.families.get_mut(&family).unwrap();
    record.sources.insert(
        ProviderId::new("nexus"),
        SourceMod {
            mod_id: "7001".to_string(),
            part_id: None,
            part_name: None,
            part_category: None,
            meta: Some(SourceMeta::from(NexusMeta {
                name: Some("Example Cleanup Mod".to_string()),
                author: Some("example-uploader".to_string()),
                category: Some("Interface".to_string()),
                checked_at: Some(1_730_000_000),
                ..NexusMeta::default()
            })),
        },
    );

    write(&library_dir.join("modA/content/y.ws"), "new");
    library.rescan(&library_dir, &game()).unwrap();

    let record = &library.families[&family];
    assert_eq!(
        record
            .source("nexus")
            .and_then(|source| source.mod_id.parse::<u32>().ok()),
        Some(7001)
    );
    assert_eq!(
        record
            .source("nexus")
            .and_then(|source| source.meta.as_ref())
            .and_then(|meta| meta.author.as_deref()),
        Some("example-uploader")
    );
}

#[test]
fn typed_info_wins_over_info_json_and_survives_a_rescan() {
    let base = scratch("typed-info");
    let library_dir = base.join("library");
    write(
        &library_dir.join("modA/info.json"),
        r#"{"modName": "Mod A", "version": "1.0", "author": "Anna"}"#,
    );
    write(&library_dir.join("modA/content/x.ws"), "a");
    let mut library = Library::load(&base.join(INDEX_FILE), &library_dir, &game()).unwrap();
    let edit = ModInfoEdit {
        name: None,
        version: Some("1.1-fix".to_string()),
        author: None,
    };
    let mut info = mod_info(&library_dir.join("modA"), "modA");
    edit.apply(&mut info);
    let version = library.versions.get_mut("modA").unwrap();
    version.info = info;
    version.edit = edit;

    write(&library_dir.join("modA/content/y.ws"), "new");
    library.rescan(&library_dir, &game()).unwrap();

    let info = &library.versions["modA"].info;
    assert_eq!(info.version.as_deref(), Some("1.1-fix"));
    assert_eq!(info.name, "Mod A");
    assert_eq!(info.author.as_deref(), Some("Anna"));
}

#[test]
fn a_nexus_archive_name_wins_over_info_json_and_a_stored_version_survives_a_rescan() {
    let base = scratch("nexus-name");
    let library_dir = base.join("library");
    write(
        &library_dir.join("modA/info.json"),
        r#"{"modName": "example-cook-name", "version": "1.0"}"#,
    );
    write(&library_dir.join("modA/content/x.ws"), "a");
    let mut library = Library::load(&base.join(INDEX_FILE), &library_dir, &game()).unwrap();
    let archive = "(Remastered) Example Mod 7002 5.01.2 2026-10-20T15-06Z example-uploader-2.zip";
    library.archives.insert(
        "sha-a".to_string(),
        Archive {
            original_name: archive.to_string(),
            file: archive.to_string(),
            source_files: std::collections::BTreeMap::new(),
            source_parts: std::collections::BTreeMap::new(),
            named: crate::providers::archive_name_from(&["nexus"], archive),
            skipped: Vec::new(),
            skipped_total: 0,
        },
    );
    let version = library.versions.get_mut("modA").unwrap();
    version.archive = Some("sha-a".to_string());
    version.parsed_sources =
        std::collections::BTreeMap::from([(ProviderId::new("nexus"), "7002".to_string())]);

    library.rescan(&library_dir, &game()).unwrap();
    let info = &library.versions["modA"].info;
    assert_eq!(info.name, "(Remastered) Example Mod");
    // The label names the mod, but a version already stored is kept: a rescan only fills a missing one. Here the
    // record came from `info.json`, so that is the version the rescan preserves.
    assert_eq!(info.version.as_deref(), Some("1.0"));

    let version = library.versions.get_mut("modA").unwrap();
    version.edit = ModInfoEdit {
        name: None,
        version: Some("5.01.2-mine".to_string()),
        author: None,
    };
    library.rescan(&library_dir, &game()).unwrap();
    let info = &library.versions["modA"].info;
    assert_eq!(info.name, "(Remastered) Example Mod");
    assert_eq!(info.version.as_deref(), Some("5.01.2-mine"));
}
