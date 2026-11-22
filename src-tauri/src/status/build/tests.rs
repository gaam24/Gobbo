//! Tests of the game status: the queries the window reads, against a library on disk.

use super::*;
use crate::domain::library::Archive;
use crate::domain::library::ModInfo;
use crate::domain::library::ModVersion;
use crate::status::mod_views::version_view;
use crate::test_support::scratch;
use crate::test_support::write;

#[test]
fn a_tool_is_found_only_inside_the_game_folder() {
    let base = scratch!("tools");
    let _ = std::fs::remove_dir_all(&base);
    let program = base.join("Tool").join("tool.exe");
    std::fs::create_dir_all(program.parent().unwrap()).unwrap();
    std::fs::write(&program, "x").unwrap();
    let root = base.display().to_string();

    assert_eq!(
        tool_folder(&root, "Tool/tool.exe"),
        Some(base.join("Tool").display().to_string())
    );
    assert_eq!(tool_folder(&root, "Missing/tool.exe"), None);
    assert_eq!(tool_folder(&root, "../tool.exe"), None);
    assert_eq!(tool_folder("", "Tool/tool.exe"), None);

    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn a_tool_path_outside_a_missing_game_folder_is_not_found() {
    let base = scratch!("tool-absolute");
    let program = base.join("Tool").join("tool.exe");
    write(&program, "x");
    let missing_root = base.join("no-game").display().to_string();

    assert_eq!(
        tool_folder(&missing_root, &program.display().to_string()),
        None
    );
}

fn version(folder: &str, name: &str) -> ModVersion {
    crate::test_support::mod_version(folder, folder, name, None)
}

fn listed(folder: &str) -> ProfileMod {
    crate::test_support::profile_mod(folder, "", true)
}

#[test]
fn readiness_asks_only_for_the_folders_that_are_missing() {
    let base = scratch!("readiness");
    let game_root = base.join("game");
    let settings_dir = base.join("settings");
    std::fs::create_dir_all(&game_root).unwrap();
    std::fs::create_dir_all(&settings_dir).unwrap();
    let game = crate::test_support::games::slot();
    let data = crate::context::Data::new(base.join("data"));

    let empty = readiness(&data, &game, &Settings::default(), false);
    assert!(!empty.game_ready);
    assert!(!empty.settings_ready);
    assert!(empty.mods_ready);

    let set = Settings {
        game_root: game_root.display().to_string(),
        settings_dir: settings_dir.display().to_string(),
        ..Settings::default()
    };
    let ready = readiness(&data, &game, &set, false);
    assert!(ready.game_ready);
    assert!(ready.settings_ready);
    assert!(ready.mods_ready);

    let overridden = Settings {
        mods_root: base.join("no-mods").display().to_string(),
        ..set
    };
    assert!(!readiness(&data, &game, &overridden, false).mods_ready);

    let external = crate::test_support::games::external();
    let ready = readiness(&data, &external, &Settings::default(), false);
    assert_eq!(ready.mods_ready, !ready.mods_root.is_empty());
}

#[test]
fn a_profile_mod_is_missing_only_when_neither_the_index_nor_the_disk_has_it() {
    let mut library = Library::default();
    library
        .versions
        .insert("indexed".into(), version("indexed", "Indexed"));
    library
        .versions
        .insert("index_only".into(), version("index_only", "Index only"));
    let disk = ["indexed".to_string(), "disk_only".to_string()];
    let mods = [
        listed("indexed"),
        listed("index_only"),
        listed("disk_only"),
        listed("gone"),
    ];

    let problems = index_problems(&library, &mods, &library.check_folders(&disk));

    let missing: Vec<&str> = problems
        .iter()
        .filter(|problem| problem.code == "library.mod_missing")
        .filter_map(|problem| problem.params.get("folder"))
        .map(String::as_str)
        .collect();
    assert_eq!(missing, ["gone"]);
}

#[test]
fn a_version_is_named_by_its_archive_only_when_the_info_has_no_name_of_its_own() {
    let archive = "Example Cleanup Mod 7001 1.2.0 2026-10-23T17-21Z example-uploader.zip";
    let label = crate::providers::archive_name_from(&[], archive).unwrap();
    let archive_name = label.name.clone().unwrap();
    let mut library = Library::default();
    library.archives.insert(
        "sha".into(),
        Archive {
            original_name: archive.into(),
            file: "archive.zip".into(),
            source_files: std::collections::BTreeMap::new(),
            source_parts: std::collections::BTreeMap::new(),
            named: Some(label),
            skipped: Vec::new(),
            skipped_total: 0,
        },
    );
    let mut folder_only = version("mod-folder", "mod-folder");
    folder_only.archive = Some("sha".into());
    library
        .versions
        .insert("mod-folder".into(), folder_only.clone());
    let profiles = Profiles::with_default(Vec::new());
    let profiles_view = crate::domain::profiles::ProfilesView::new(&profiles);

    assert_eq!(
        version_view(&library, &profiles_view, &folder_only).name,
        archive_name
    );
    let named = ModVersion {
        info: ModInfo {
            name: "Own name".into(),
            ..ModInfo::default()
        },
        ..folder_only
    };
    assert_eq!(
        version_view(&library, &profiles_view, &named).name,
        "Own name"
    );
}

#[cfg(feature = "bench")]
#[test]
#[ignore = "uses the benchmark fixture"]
fn a_patch_with_the_same_revisions_is_empty() {
    use crate::status::patch;
    use crate::status::views::KnownRows;

    let fixture = crate::bench_api::StatusFixture::new(100, 0);
    let full = fixture.status_value();
    let known = KnownRows {
        ids: full.mods.iter().map(|view| view.id.clone()).collect(),
        revisions: full.revisions.clone(),
    };
    let empty = patch(fixture.status_value(), &known);
    assert!(empty.patch);
    assert!(empty.mods.is_empty());
    assert!(empty.revisions.is_empty());
    assert!(empty.removed.is_empty());
    assert!(empty.order.is_none());

    // A window without revisions gets the whole list and its order, but not the removed ids.
    let fresh = patch(
        fixture.status_value(),
        &KnownRows {
            ids: Vec::new(),
            revisions: Vec::new(),
        },
    );
    assert_eq!(fresh.mods.len(), 100);
    assert_eq!(fresh.order.as_ref().map(Vec::len), Some(100));
}

#[test]
fn a_status_read_does_not_create_the_library_or_archive_folders() {
    crate::test_support::games::install();
    let id = crate::test_support::games::SLOT_ID;
    let base = scratch!("status-read-no-folders");
    let root = Data::new(base.join("data"));
    let data = root.game(id);
    crate::context::settings::save_settings(&data, id, Settings::default()).unwrap();
    assert!(!data.library().exists());
    assert!(!data.archives().exists());

    // A pure read leaves the data folder as it was, even on the first look at a game.
    crate::status::status(&root, id).unwrap();
    assert!(!data.library().exists());
    assert!(!data.archives().exists());

    // The change path owns the write, so the folders exist once a game opens.
    crate::status::sync_profiles(&root, id).unwrap();
    assert!(data.library().is_dir());
    assert!(data.archives().is_dir());
}

#[test]
fn the_subtitle_names_the_store_or_the_steam_app_id() {
    let ids = [1001];
    assert_eq!(subtitle_for("steam", &ids), "Steam 1001");
    assert_eq!(subtitle_for("gog", &[]), "GOG");
    assert_eq!(subtitle_for("epic", &[]), "Epic");
    assert_eq!(subtitle_for("heroic", &[]), "Heroic");
    assert_eq!(subtitle_for("lutris", &[]), "Lutris");
    assert_eq!(subtitle_for("manual", &[]), "Manual");
    assert_eq!(subtitle_for("", &ids), "Steam 1001");
    assert_eq!(subtitle_for("", &[]), "Manual");
    assert_eq!(subtitle_for("unknown", &ids), "Steam 1001");
    assert_eq!(subtitle_for("unknown", &[]), "");
}
