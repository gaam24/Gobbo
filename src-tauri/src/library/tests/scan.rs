//! Scanning the folder: the first load, the index read back, rescans and refreshes.

use super::*;
use crate::context::library::INDEX_FILE;
use crate::domain::fomod::Choice;
use crate::domain::source::ProviderId;
use std::fs;

#[test]
fn first_load_scans_the_folder_and_saves_the_index() {
    let base = scratch("first");
    let library_dir = base.join("library");
    write(&library_dir.join("modA/content/x.ws"), "a");
    write(&library_dir.join(".importing/junk.txt"), "hidden");
    let index = base.join(INDEX_FILE);

    let library = Library::load(&index, &library_dir, &game()).unwrap();
    assert_eq!(library.versions.keys().collect::<Vec<_>>(), ["modA"]);
    assert_eq!(
        library.versions["modA"]
            .files
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["content/x.ws"]
    );
    assert_eq!(library.versions["modA"].slot, "Mods");
    assert!(index.is_file());
}

#[test]
fn a_slot_the_game_no_longer_has_is_replaced_when_the_index_loads() {
    let base = scratch("old-slot");
    let library_dir = base.join("library");
    write(&library_dir.join("modA/content/x.ws"), "a");
    let index = base.join(INDEX_FILE);
    let mut library = Library::load(&index, &library_dir, &game()).unwrap();
    library.versions.get_mut("modA").unwrap().slot = "gone".to_string();
    library.save(&index).unwrap();

    let again = Library::load(&index, &library_dir, &game()).unwrap();
    assert_eq!(again.versions["modA"].slot, "Mods");
}

#[test]
fn index_is_read_back_without_touching_the_folder() {
    let base = scratch("no-scan");
    let library_dir = base.join("library");
    write(&library_dir.join("modA/content/x.ws"), "a");
    let index = base.join(INDEX_FILE);
    Library::load(&index, &library_dir, &game()).unwrap();

    write(&library_dir.join("modA/content/y.ws"), "b");
    let again = Library::load(&index, &library_dir, &game()).unwrap();
    assert_eq!(
        again.versions["modA"]
            .files
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["content/x.ws"]
    );

    let mut rescanned = again;
    rescanned.rescan(&library_dir, &game()).unwrap();
    assert_eq!(
        rescanned.versions["modA"]
            .files
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["content/x.ws", "content/y.ws"]
    );
}

#[test]
fn rescan_keeps_archive_links_and_drops_folders_that_are_gone() {
    let base = scratch("rescan");
    let library_dir = base.join("library");
    let mut library = load_mods(&base, &["modA", "modB"]);
    link_archive_a(&mut library);

    fs::remove_dir_all(library_dir.join("modB")).unwrap();
    library.rescan(&library_dir, &game()).unwrap();

    assert_eq!(library.versions.keys().collect::<Vec<_>>(), ["modA"]);
    assert_eq!(library.archive_name("modA"), Some("A-1.zip"));
}

#[test]
fn refresh_rescans_only_enabled_versions() {
    let base = scratch("refresh");
    let library_dir = base.join("library");
    let mut library = load_mods(&base, &["modA", "modB"]);
    write(&library_dir.join("modA/content/y.ws"), "new");
    write(&library_dir.join("modB/content/y.ws"), "new");

    library
        .refresh(
            &library_dir,
            &game(),
            &[listed("modA", true), listed("modB", false)],
        )
        .unwrap();
    assert_eq!(library.versions["modA"].files.len(), 2);
    assert_eq!(library.versions["modB"].files.len(), 1);
}

#[test]
fn refresh_keeps_the_metadata_of_a_known_version() {
    let base = scratch("refresh-metadata");
    let library_dir = base.join("library");
    let mut library = load_mods(&base, &["modA"]);
    let provider = ProviderId::new("nexus");
    let version = library.versions.get_mut("modA").unwrap();
    version.info = ModInfo {
        name: "Custom Name".to_string(),
        version: Some("1.2.3".to_string()),
        author: Some("Author".to_string()),
        description: Some("Description".to_string()),
    };
    version.edit = ModInfoEdit {
        name: None,
        version: Some("1.2.3-mine".to_string()),
        author: None,
    };
    version.parsed_sources = std::collections::BTreeMap::from([(provider.clone(), "4242".into())]);
    version.parsed_parts = std::collections::BTreeMap::from([(provider, "part".into())]);
    version.archive = Some("sha-a".to_string());
    version.installed_at = 42;
    version.fomod = vec![Choice {
        step: 0,
        group_index: 0,
        group: "Group".to_string(),
        option: "Option".to_string(),
    }];
    version.fomod_installer = true;
    let before = library.versions["modA"].clone();
    write(&library_dir.join("modA/content/y.ws"), "new");

    library
        .refresh(&library_dir, &game(), &[listed("modA", true)])
        .unwrap();

    let after = &library.versions["modA"];
    assert_eq!(after.info, before.info);
    assert_eq!(after.edit, before.edit);
    assert_eq!(after.parsed_sources, before.parsed_sources);
    assert_eq!(after.parsed_parts, before.parsed_parts);
    assert_eq!(after.archive, before.archive);
    assert_eq!(after.fomod, before.fomod);
    assert!(after.fomod_installer);
    assert_eq!(after.installed_at, before.installed_at);
    assert_eq!(after.family, before.family);
    assert_eq!(after.slot, before.slot);
    assert_eq!(after.files.len(), 2);
}

/// A scan repairs a slot the game no longer has, but keeps an empty slot and a still-valid slot even when the scan
/// derives a different one from the folder name. `refresh` (the known-record branch) and `rescan` (through
/// `keep_known`) both follow this rule.
#[test]
fn scanning_repairs_an_obsolete_slot_and_keeps_empty_or_valid_ones() {
    let base = scratch("slots");
    let library_dir = base.join("library");
    let mut library = load_mods(&base, &["obsolete", "empty", "valid"]);
    // A second slot lets a stored, valid slot differ from the one the scan derives from the folder name.
    let mut multi = game();
    let mut other = multi.deploy.slots[0].clone();
    other.dir = "Other".to_string();
    multi.deploy.slots.push(other);
    let set_slots = |library: &mut Library| {
        library.versions.get_mut("obsolete").unwrap().slot = "gone".to_string();
        library.versions.get_mut("empty").unwrap().slot = String::new();
        library.versions.get_mut("valid").unwrap().slot = "Other".to_string();
    };
    let listed_all = || {
        [
            listed("obsolete", true),
            listed("empty", true),
            listed("valid", true),
        ]
    };

    set_slots(&mut library);
    library
        .refresh(&library_dir, &multi, &listed_all())
        .unwrap();
    assert_eq!(library.versions["obsolete"].slot, "Mods");
    assert_eq!(library.versions["empty"].slot, "");
    assert_eq!(library.versions["valid"].slot, "Other");

    set_slots(&mut library);
    library.rescan(&library_dir, &multi).unwrap();
    assert_eq!(library.versions["obsolete"].slot, "Mods");
    assert_eq!(library.versions["empty"].slot, "");
    assert_eq!(library.versions["valid"].slot, "Other");
}

#[test]
fn refresh_refuses_an_enabled_version_whose_folder_is_missing() {
    let base = scratch("missing");
    let library_dir = base.join("library");
    write(&library_dir.join("modA/content/x.ws"), "a");
    let mut library = Library::load(&base.join(INDEX_FILE), &library_dir, &game()).unwrap();
    fs::remove_dir_all(library_dir.join("modA")).unwrap();

    let err = library
        .refresh(&library_dir, &game(), &[listed("modA", true)])
        .unwrap_err();
    assert_eq!(err.code, "library.mod_missing", "{err}");
    assert_eq!(err.params["folder"], "modA", "{err}");
}

#[test]
fn a_damaged_index_is_kept_broken_and_rebuilt_from_the_library() {
    let base = scratch("index-broken");
    let library_dir = base.join("library");
    write(&library_dir.join("modA/content/x.ws"), "a");
    let index = base.join(INDEX_FILE);
    fs::write(&index, "{ not json").unwrap();

    let library = Library::load(&index, &library_dir, &game()).unwrap();
    assert!(library.versions.contains_key("modA"));
    assert!(index.is_file(), "a fresh index was written");
    assert!(
        base.join("library.json.broken").is_file(),
        "the damaged file was kept"
    );
}

#[test]
fn an_index_from_a_newer_gobbo_is_refused_instead_of_overwritten() {
    let base = scratch("index-newer");
    let library_dir = base.join("library");
    write(&library_dir.join("modA/content/x.ws"), "a");
    let index = base.join(INDEX_FILE);
    let mut value = serde_json::to_value(Library::default()).unwrap();
    value["schema_version"] = serde_json::Value::from(999);
    let text = serde_json::to_string(&value).unwrap();
    fs::write(&index, &text).unwrap();

    let error = Library::load(&index, &library_dir, &game()).unwrap_err();
    assert_eq!(error.code, "state.schema_newer");
    assert_eq!(
        fs::read_to_string(&index).unwrap(),
        text,
        "the newer file is untouched"
    );
}
