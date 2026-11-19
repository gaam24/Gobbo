//! Importing archives: the Nexus names and ids, the copy kept in the library, duplicates and progress.

use std::collections::BTreeSet;
use std::io;

use super::*;
use crate::import::model::ImportStage;
use crate::library::rescan;
use crate::status::views::Status;
use crate::test_support::import;
use sha2::Sha256;

/// Imports one archive with one script into the mirror fixture, and returns the status after it.
fn imported_status(name: &str, archive: &str, script: &str) -> Status {
    let f = mirror_fixture(name);
    let zip = f.zip(archive, &[(script, "x")]);
    import(&f.data, MIRROR_GAME, &zip, None, &mut |_, _| {}).unwrap();
    status(&f.data, MIRROR_GAME).unwrap()
}

#[test]
fn a_thunderstore_archive_shows_the_version_from_its_file_name() {
    let f = mirror_fixture("thunderstore-version");
    let zip = f.zip(
        "owner_name-package_name-1.0.0.zip",
        &[("r6/scripts/ThunderMod/a.reds", "x")],
    );
    import(&f.data, MIRROR_GAME, &zip, None, &mut |_, _| {}).unwrap();
    let status = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(status.mods[0].info.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        status.mods[0]
            .sources
            .iter()
            .find(|source| source.provider == "thunderstore")
            .map(|source| source.mod_id.as_str()),
        Some("owner_name-package_name")
    );
}

#[test]
fn a_recipe_display_name_does_not_rename_the_imported_mod_or_its_folder() {
    let f = mirror_fixture("import-recipe-name");
    let zip = f.zip(
        "bundle_CDTeWEvack.zip",
        &[("r6/scripts/BundledMod/a.reds", "x")],
    );
    // A recipe has no channel to name the library record any more: the archive's file stem decides both. Reinstalling
    // the folder must still find its mod.
    let request = crate::import::ImportRequest::new(&zip);
    let view =
        crate::import::import_archive(&f.data, MIRROR_GAME, request, None, &mut |_, _| {}).unwrap();
    assert_eq!(view.mods[0].name, "bundle_CDTeWEvack");
    assert_eq!(view.folders[0], "bundle_CDTeWEvack");
    crate::import::reinstall_mod(&f.data, MIRROR_GAME, &view.folders[0]).unwrap();
}

#[test]
fn an_unplaced_file_is_reported_and_kept_in_the_archive_report() {
    let f = fixture("import-skipped-config");
    let zip = f.zip(
        "Config Mod.zip",
        &[
            ("Mods/ConfigMod/content/x.ws", "x"),
            ("settings.ini", "[settings]"),
        ],
    );
    let view = import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    assert_eq!(view.skipped, 1);
    assert!(view.skipped_files.iter().any(|file| {
        file.path == "settings.ini" && file.reason == crate::domain::library::SkipReason::NoRule
    }));

    let state = status(&f.data, SLOT_GAME).unwrap();
    let folder = state.mods[0].folder.clone();
    assert_eq!(state.mods[0].skipped, 1);
    let report = crate::import::archive_report(&f.data, SLOT_GAME, &folder).unwrap();
    assert!(
        report
            .skipped
            .iter()
            .any(|file| file.path == "settings.ini")
    );
    assert_eq!(report.skipped_total, 1);
    assert!(report.missing.is_empty());
    assert!(report.extra.is_empty());
    assert_eq!(report.expected, 1);
    assert_eq!(report.installed, 1);
}

#[test]
fn the_archive_report_counts_every_unplaced_file_and_zero_when_there_are_none() {
    let two = fixture("import-two-skipped");
    let zip = two.zip(
        "Config Pack.zip",
        &[
            ("Mods/ConfigMod/content/x.ws", "x"),
            ("settings.ini", "[settings]"),
            ("preferences.ini", "[preferences]"),
        ],
    );
    let view = import(&two.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    assert_eq!(view.skipped, 2);
    assert_eq!(view.skipped_files.len(), 2);

    let state = status(&two.data, SLOT_GAME).unwrap();
    let folder = state.mods[0].folder.clone();
    // This total is stored in the index at import time, so a constant there is wrong for any other archive.
    assert_eq!(state.mods[0].skipped, 2);
    let report = crate::import::archive_report(&two.data, SLOT_GAME, &folder).unwrap();
    assert_eq!(report.skipped_total, 2);
    assert_eq!(report.skipped.len(), 2);
    assert!(
        report
            .skipped
            .iter()
            .all(|file| file.reason == crate::domain::library::SkipReason::NoRule)
    );

    let none = fixture("import-no-skipped");
    let zip = none.zip("Clean Mod.zip", &[("Mods/CleanMod/content/x.ws", "x")]);
    import(&none.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    assert_eq!(status(&none.data, SLOT_GAME).unwrap().mods[0].skipped, 0);
}

#[test]
fn an_archive_report_finds_changed_missing_and_extra_files() {
    let f = fixture("import-archive-report");
    let zip = f.zip("Report Mod.zip", &[("Mods/ReportMod/content/x.ws", "x")]);
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    let state = status(&f.data, SLOT_GAME).unwrap();
    let folder = state.mods[0].folder.clone();
    let dir = f.data.game(SLOT_GAME).library().join(&folder);
    let file = dir.join("content/x.ws");

    crate::test_support::write(&file, "longer");
    let report = crate::import::archive_report(&f.data, SLOT_GAME, &folder).unwrap();
    assert!(report.changed.contains(&"content/x.ws".to_string()));
    assert!(report.missing.is_empty());

    fs::remove_file(&file).unwrap();
    crate::test_support::write(&dir.join("extra.txt"), "extra");
    let report = crate::import::archive_report(&f.data, SLOT_GAME, &folder).unwrap();
    assert!(report.missing.contains(&"content/x.ws".to_string()));
    assert!(report.extra.contains(&"extra.txt".to_string()));
}

#[test]
fn an_integrity_check_detects_a_same_size_edit_after_import() {
    let f = fixture("import-integrity");
    let zip = f.zip(
        "Integrity Mod.zip",
        &[("Mods/IntegrityMod/content/x.ws", "abc")],
    );
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    let state = status(&f.data, SLOT_GAME).unwrap();
    let folder = state.mods[0].folder.clone();
    let verify = || crate::library::integrity::verify(&f.data, SLOT_GAME, &folder).unwrap();

    assert_eq!(
        verify(),
        crate::library::integrity::IntegrityStatus::Matches
    );

    // Same length, different content: a size-only report would miss this.
    let file = f
        .data
        .game(SLOT_GAME)
        .library()
        .join(&folder)
        .join("content/x.ws");
    crate::test_support::write(&file, "ABC");
    assert_eq!(
        verify(),
        crate::library::integrity::IntegrityStatus::Changed
    );

    // A rescan keeps the recorded baseline, so the change is still detected.
    crate::library::rescan(&f.data, SLOT_GAME).unwrap();
    assert_eq!(
        verify(),
        crate::library::integrity::IntegrityStatus::Changed
    );
}

#[test]
fn a_thunderstore_archive_shows_its_info_json_name_and_its_archive_version_after_import_and_rescan()
{
    let f = fixture("thunderstore-import-version");
    let zip = f.zip(
        "example_owner-example_mod-2.1.0.zip",
        &[
            ("Mods/modThunder/info.json", r#"{"modName":"Example Name"}"#),
            ("Mods/modThunder/content/x.ws", "x"),
        ],
    );
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    let imported = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(imported.mods[0].info.name, "Example Name");
    assert_eq!(imported.mods[0].info.version.as_deref(), Some("2.1.0"));

    rescan(&f.data, SLOT_GAME).unwrap();
    let rescanned = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(rescanned.mods[0].info.name, "Example Name");
    assert_eq!(rescanned.mods[0].info.version.as_deref(), Some("2.1.0"));
}

#[test]
fn an_older_nexus_archive_shows_its_name_and_id_and_keeps_its_folder() {
    let status = imported_status(
        "nexus-old-status",
        "Sort Everything-1234-1-1-1584112564.zip",
        "r6/scripts/SortEverything/a.reds",
    );
    let listed = &status.mods[0];
    assert_eq!(listed.folder, "Sort Everything-1234-1-1-1584112564");
    assert_eq!(listed.info.name, "Sort Everything");
    assert_eq!(listed.info.version.as_deref(), Some("1.1"));
    assert_eq!(
        listed
            .sources
            .iter()
            .find(|source| source.provider == "nexus")
            .map(|source| source.mod_id.as_str()),
        Some("1234")
    );
}

#[test]
fn a_nexus_archive_shows_its_name_version_and_id() {
    let status = imported_status(
        "nexus-status",
        "Remove Mods Menu 35686 1.2.0 2026-10-23T17-21Z Q6xF6llIa.zip",
        "r6/scripts/RemoveModsMenu/RemoveModsMenu.reds",
    );
    let listed = &status.mods[0];
    assert_eq!(
        listed.folder,
        "Remove Mods Menu 35686 1.2.0 2026-10-23T17-21Z Q6xF6llIa"
    );
    assert_eq!(listed.info.name, "Remove Mods Menu");
    assert_eq!(listed.info.version.as_deref(), Some("1.2.0"));
    assert_eq!(
        listed
            .sources
            .iter()
            .find(|source| source.provider == "nexus")
            .map(|source| source.mod_id.as_str()),
        Some("35686")
    );
}

#[test]
fn two_parts_of_one_nexus_page_become_two_mods() {
    let f = mirror_fixture("nexus-parts");
    let first = f.zip(
        "Addon A 225 1.0 2026-10-23T17-21Z A.zip",
        &[("r6/scripts/AddonA/a.reds", "a")],
    );
    let second = f.zip(
        "Addon B 225 1.0 2026-10-23T17-22Z B.zip",
        &[("r6/scripts/AddonB/b.reds", "b")],
    );
    let source = |part: &str, name: &str| {
        crate::domain::source::SourceRef::new("nexus", "225")
            .with_part(part)
            .with_file(format!("file-{part}"))
            .with_part_meta(Some(name), Some("main"))
    };
    crate::test_support::import_from(
        &f.data,
        MIRROR_GAME,
        &first,
        None,
        Some(source("100", "Addon A")),
        &mut |_, _| {},
    )
    .unwrap();
    crate::test_support::import_from(
        &f.data,
        MIRROR_GAME,
        &second,
        None,
        Some(source("200", "Addon B")),
        &mut |_, _| {},
    )
    .unwrap();

    let status = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(status.mods.len(), 2);
    let ids: Vec<&str> = status.mods.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, ["nexus:225", "nexus:225#200"]);
    assert_eq!(status.mods[0].page_id.as_deref(), Some("nexus:225"));
    assert_eq!(status.mods[1].page_id.as_deref(), Some("nexus:225"));
    assert_eq!(status.mods[0].part_id.as_deref(), Some("100"));
    assert_eq!(status.mods[1].part_id.as_deref(), Some("200"));
    assert!(status.mods.iter().all(|item| !item.enabled));
}

#[test]
fn a_new_version_of_the_same_part_joins_its_family() {
    let f = mirror_fixture("nexus-part-version");
    let source = crate::domain::source::SourceRef::new("nexus", "225")
        .with_part("100")
        .with_part_meta(Some("Addon A"), Some("main"));
    let mut views = Vec::new();
    for (name, text) in [
        ("Addon A 225 1.0 2026-10-23T17-21Z A.zip", "one"),
        ("Addon A 225 1.1 2026-11-01T09-00Z B.zip", "two"),
    ] {
        let zip = f.zip(name, &[("r6/scripts/AddonA/a.reds", text)]);
        views.push(
            crate::test_support::import_from(
                &f.data,
                MIRROR_GAME,
                &zip,
                None,
                Some(source.clone().with_file(format!("file-{text}"))),
                &mut |_, _| {},
            )
            .unwrap(),
        );
    }

    let status = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(status.mods.len(), 1);
    assert_eq!(status.mods[0].id, "nexus:225");
    assert_eq!(status.mods[0].version_count, 2);
    // The view names the family and the folder, so the window can offer the imported version instead of guessing.
    assert_eq!(views[1].mods.len(), 1);
    assert_eq!(views[1].mods[0].family, "nexus:225");
    assert_eq!(views[1].mods[0].name, "Addon A");
    assert_eq!(views[1].mods[0].folder, views[1].folders[0]);
}

#[test]
fn a_new_part_joins_its_page_group_in_the_profile() {
    let f = mirror_fixture("nexus-parts-order");
    let part = |part: &str, name: &str| {
        crate::domain::source::SourceRef::new("nexus", "225")
            .with_part(part)
            .with_part_meta(Some(name), Some("main"))
    };
    for (name, source, path) in [
        (
            "Part A.zip",
            Some(part("100", "Part A")),
            "r6/scripts/A/a.reds",
        ),
        ("Other.zip", None, "r6/scripts/Other/o.reds"),
        (
            "Part B.zip",
            Some(part("200", "Part B")),
            "r6/scripts/B/b.reds",
        ),
    ] {
        let zip = f.zip(name, &[(path, "x")]);
        crate::test_support::import_from(&f.data, MIRROR_GAME, &zip, None, source, &mut |_, _| {})
            .unwrap();
        status(&f.data, MIRROR_GAME).unwrap();
    }

    let status = status(&f.data, MIRROR_GAME).unwrap();
    let ids: Vec<&str> = status.mods.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(
        ids,
        ["nexus:225", "nexus:225#200", "local:other"],
        "the second part sits next to the first, not at the end"
    );
}

#[test]
fn deleting_a_page_removes_every_part() {
    let f = mirror_fixture("nexus-delete-page");
    for (part, path) in [
        ("100", "r6/scripts/A/a.reds"),
        ("200", "r6/scripts/B/b.reds"),
    ] {
        let zip = f.zip(&format!("Part {part}.zip"), &[(path, "x")]);
        let source = crate::domain::source::SourceRef::new("nexus", "225")
            .with_part(part)
            .with_part_meta(Some(part), Some("main"));
        crate::test_support::import_from(
            &f.data,
            MIRROR_GAME,
            &zip,
            None,
            Some(source),
            &mut |_, _| {},
        )
        .unwrap();
        status(&f.data, MIRROR_GAME).unwrap();
    }

    crate::deploy::remove::delete_page(&f.data, MIRROR_GAME, "nexus:225").unwrap();
    assert!(status(&f.data, MIRROR_GAME).unwrap().mods.is_empty());
}

#[test]
fn a_batch_imports_every_group_and_saves_the_index_once() {
    let f = mirror_fixture("batch-import");
    let requests: Vec<crate::import::ImportRequest> = (0..9)
        .map(|number| {
            let entry = format!("r6/scripts/Batch{number}/b.reds");
            let zip = f.zip(&format!("Batch {number}.zip"), &[(entry.as_str(), "x")]);
            crate::import::ImportRequest::new(zip)
        })
        .collect();

    let results =
        crate::import::import_batch(&f.data, MIRROR_GAME, requests, &mut |_, _| {}, None).unwrap();
    assert_eq!(results.len(), 9);
    assert!(results.iter().all(std::result::Result::is_ok));

    // A new handle without the read-through cache proves the last group reached the file.
    let game = crate::games::load(MIRROR_GAME).unwrap();
    let data = crate::context::Data::new(f.base.join("data")).game(MIRROR_GAME);
    let library = crate::context::load_library(&data, &game).unwrap();
    assert_eq!(library.versions.len(), 9);
    assert_eq!(library.archives.len(), 9);
}

#[test]
fn import_adds_mods_to_the_profile_disabled() {
    let f = fixture("import");
    f.import_two_mods();

    let status = status(&f.data, SLOT_GAME).unwrap();
    let folders: Vec<&str> = status.mods.iter().map(|m| m.folder.as_str()).collect();
    assert_eq!(folders, ["modA", "modB"]);
    assert!(status.mods.iter().all(|m| !m.enabled && m.files == 1));
    assert!(!status.deployed);
}

#[test]
fn info_json_from_the_archive_names_the_mod() {
    let f = fixture("info");
    f.import_two_mods();
    let info = f
        .data
        .game(SLOT_GAME)
        .library()
        .join("modA/content/info.json");
    fs::write(
        &info,
        r#"{"modName": "Alpha", "version": "1.2", "author": "TestAuthor"}"#,
    )
    .unwrap();

    // The index remembered `info.json` from the import, so a change on disk appears after a scan.
    let before = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(before.mods[0].info.name, "modA");
    rescan(&f.data, SLOT_GAME).unwrap();

    let status = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(status.mods[0].info.name, "Alpha");
    assert_eq!(status.mods[0].info.version.as_deref(), Some("1.2"));
    assert_eq!(status.mods[0].info.author.as_deref(), Some("TestAuthor"));
    assert_eq!(status.mods[1].info.name, "modB");
}

#[test]
fn imported_archive_and_list_survive_a_restart() {
    let f = fixture("restart");
    f.import_two_mods();

    // "Restart": a new data object in the same folder.
    let after_restart = Data::new(f.base.join("data"));
    let view = status(&after_restart, SLOT_GAME).unwrap();
    let folders: Vec<&str> = view.mods.iter().map(|m| m.folder.as_str()).collect();
    assert_eq!(folders, ["modA", "modB"]);
    assert!(
        view.mods
            .iter()
            .all(|m| !m.enabled && m.files == 1 && m.archive.is_some())
    );
    assert_eq!(
        fs::read_dir(f.data.game(SLOT_GAME).archives())
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn the_same_archive_is_refused_the_second_time() {
    let f = fixture("duplicate");
    let zip = f.zip("pack.zip", &[("Mods/modA/content/x.ws", "from A")]);
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();

    let err = import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap_err();
    assert_eq!(err.code, "install.already_in_library", "{err}");
}

#[test]
fn an_archive_over_the_limits_keeps_the_archive_code() {
    // The count and byte checks are unit-tested in `infra::archive`; this keeps the import boundary from turning the
    // code into the generic archive error.
    for (archive_error, code, actual) in [
        (
            crate::infra::archive::Error::TooManyEntries {
                count: 200_001,
                limit: 200_000,
            },
            "archive.too_many_entries",
            ("count", "200001"),
        ),
        (
            crate::infra::archive::Error::TooLarge {
                bytes: 33,
                limit: 32,
            },
            "archive.too_large",
            ("bytes", "33"),
        ),
    ] {
        let error: AppError = crate::import::install::InstallError::Archive(archive_error).into();
        assert_eq!(error.code, code);
        assert_eq!(error.params[actual.0], actual.1);
    }
}

#[test]
fn a_fomod_with_no_picked_options_keeps_the_required_files_and_leaves_the_rest_out() {
    let f = fixture("fomod-empty");
    let zip = f.zip(
        "installer.zip",
        &[
            (
                "fomod/ModuleConfig.xml",
                r#"<config>
                    <moduleName>Installer</moduleName>
                    <installSteps>
                        <installStep name="Files">
                            <optionalFileGroups>
                                <group name="Core" type="SelectAtLeastOne">
                                    <plugins>
                                        <plugin name="Required">
                                            <files><file source="Mods/modA/content/required.ws" destination="Mods/modA/content/required.ws"/></files>
                                            <typeDescriptor><type name="Required"/></typeDescriptor>
                                        </plugin>
                                    </plugins>
                                </group>
                            </optionalFileGroups>
                        </installStep>
                    </installSteps>
                </config>"#,
            ),
            ("Mods/modA/content/required.ws", "required"),
            ("Mods/modA/content/optional.ws", "optional"),
        ],
    );
    import(&f.data, SLOT_GAME, &zip, Some(&[]), &mut |_, _| {}).unwrap();

    let imported = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(imported.mods.len(), 1);
    assert_eq!(imported.mods[0].files, 1);
    let library = f.data.game(SLOT_GAME).library();
    let folder = library.join(&imported.mods[0].folder);
    assert!(folder.join("content/required.ws").is_file());
    assert!(!folder.join("content/optional.ws").exists());

    // The index remembers that this archive has an installer even with no picked option, so a reinstall filters again.
    crate::import::reinstall_mod(&f.data, SLOT_GAME, &imported.mods[0].folder).unwrap();
    let reinstalled = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(reinstalled.mods[0].files, 1);
    let folder = library.join(&reinstalled.mods[0].folder);
    assert!(folder.join("content/required.ws").is_file());
    assert!(!folder.join("content/optional.ws").exists());
}

#[test]
fn a_fomod_installer_with_every_source_under_the_fomod_folder_installs_at_its_destinations() {
    let f = game_fixture(
        "fomod-folder-sources",
        test_games::FULL_ROOT_ID,
        &["archive/pc/mod", "bin/x64"],
    );
    let zip = f.zip(
        "installer.zip",
        &[
            (
                "fomod/ModuleConfig.xml",
                r#"<config>
                    <moduleName>Installer</moduleName>
                    <installSteps>
                        <installStep name="Files">
                            <optionalFileGroups>
                                <group name="Core" type="SelectAtLeastOne">
                                    <plugins>
                                        <plugin name="Base">
                                            <files><file source="fomod/parts/archive/pc/mod/base.xl" destination="archive/pc/mod/base.xl"/></files>
                                            <typeDescriptor><type name="Required"/></typeDescriptor>
                                        </plugin>
                                    </plugins>
                                </group>
                            </optionalFileGroups>
                        </installStep>
                    </installSteps>
                </config>"#,
            ),
            ("fomod/parts/archive/pc/mod/base.xl", "payload"),
            ("fomod/info.xml", "metadata"),
        ],
    );

    import(
        &f.data,
        test_games::FULL_ROOT_ID,
        &zip,
        Some(&[]),
        &mut |_, _| {},
    )
    .unwrap();

    let imported = status(&f.data, test_games::FULL_ROOT_ID).unwrap();
    assert_eq!(imported.mods.len(), 1);
    assert_eq!(imported.mods[0].files, 1);
    let folder = f
        .data
        .game(test_games::FULL_ROOT_ID)
        .library()
        .join(&imported.mods[0].folder);
    assert!(folder.join("archive/pc/mod/base.xl").is_file());
    assert!(!folder.join("fomod").exists());
}

#[test]
fn the_archive_copy_keeps_the_name_of_the_zip() {
    let f = fixture("copy-name");
    let zip = f.zip(
        "Mod Name 1.2.0.zip",
        &[("Mods/modA/content/x.ws", "from A")],
    );
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();

    let status = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(
        status.mods[0].archive_file.as_deref(),
        Some("Mod Name 1.2.0.zip")
    );
    assert!(
        f.data
            .game(SLOT_GAME)
            .archives()
            .join("Mod Name 1.2.0.zip")
            .is_file()
    );
}

#[test]
fn a_second_archive_with_the_same_name_gets_the_start_of_its_digest() {
    let f = fixture("same-name");
    let first = f.zip("pack.zip", &[("Mods/modA/content/x.ws", "from A")]);
    import(&f.data, SLOT_GAME, &first, None, &mut |_, _| {}).unwrap();
    fs::rename(&first, f.base.join("pack-first.zip")).unwrap();
    let second = f.zip("pack.zip", &[("Mods/modB/content/x.ws", "from B")]);
    let digest = crate::infra::io::digest_file::<Sha256>(&second, &mut |_| {}).unwrap();
    import(&f.data, SLOT_GAME, &second, None, &mut |_, _| {}).unwrap();

    let copies: BTreeSet<String> = fs::read_dir(f.data.game(SLOT_GAME).archives())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        copies,
        BTreeSet::from([
            "pack.zip".to_string(),
            format!("pack ({}).zip", &digest[..8]),
        ])
    );
}

#[test]
fn import_reports_each_stage_from_0_to_100_in_order() {
    let f = fixture("progress");
    let zip = f.zip("pack.zip", &[("Mods/modA/content/x.ws", "from A")]);
    let mut seen = Vec::new();
    import(&f.data, SLOT_GAME, &zip, None, &mut |stage, percent| {
        seen.push((stage, percent));
    })
    .unwrap();

    let mut stages: Vec<ImportStage> = seen.iter().map(|(stage, _)| *stage).collect();
    stages.dedup();
    assert_eq!(stages, [ImportStage::Hashing, ImportStage::Extracting]);
    for stage in stages {
        let percents: Vec<u32> = seen
            .iter()
            .filter(|(s, _)| *s == stage)
            .map(|(_, percent)| *percent)
            .collect();
        assert_eq!(percents.last(), Some(&100), "{stage:?}: {percents:?}");
        assert!(
            percents.windows(2).all(|pair| pair[0] < pair[1]),
            "{stage:?}: {percents:?}"
        );
    }
}

#[test]
fn overlapping_imports_keep_every_entry_in_the_index() {
    let f = fixture("overlap");
    let first = f.zip("first.zip", &[("Mods/first/content/x.ws", "one")]);
    let second = f.zip("second.zip", &[("Mods/second/content/x.ws", "two")]);
    std::thread::scope(|scope| {
        scope.spawn(|| import(&f.data, SLOT_GAME, &first, None, &mut |_, _| {}).unwrap());
        scope.spawn(|| import(&f.data, SLOT_GAME, &second, None, &mut |_, _| {}).unwrap());
    });

    let status = status(&f.data, SLOT_GAME).unwrap();
    let mut folders: Vec<&str> = status.mods.iter().map(|m| m.folder.as_str()).collect();
    folders.sort_unstable();
    assert_eq!(folders, ["first", "second"]);
}

/// Imports each archive on a thread of its own, all at once, and returns the errors of the refused ones.
fn import_at_once<Z: AsRef<std::path::Path> + Sync>(data: &Data, zips: &[Z]) -> Vec<AppError> {
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for zip in zips {
            handles.push(
                scope.spawn(move || import(data, SLOT_GAME, zip.as_ref(), None, &mut |_, _| {})),
            );
        }
        handles
            .into_iter()
            .filter_map(|handle| handle.join().unwrap().err())
            .collect()
    })
}

#[test]
fn the_same_archive_imported_at_once_is_taken_once() {
    let f = fixture("twins");
    let first = f.zip("first.zip", &[("Mods/twin/content/x.ws", "same")]);
    let second = f.base.join("second.zip");
    fs::copy(&first, &second).unwrap();
    let errors = import_at_once(&f.data, &[first, second]);
    assert_eq!(errors.len(), 1, "one of the two is refused");
    assert_eq!(errors[0].code, "install.already_in_library");
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods.len(), 1);
}

#[test]
fn archives_of_the_same_name_imported_at_once_keep_both_copies() {
    let f = fixture("same-name-at-once");
    let first_dir = f.base.join("first");
    let second_dir = f.base.join("second");
    fs::create_dir_all(&first_dir).unwrap();
    fs::create_dir_all(&second_dir).unwrap();
    let first =
        crate::test_support::make_zip(&first_dir, "Mod.zip", &[("Mods/one/content/x.ws", "one")]);
    let second =
        crate::test_support::make_zip(&second_dir, "Mod.zip", &[("Mods/two/content/x.ws", "two")]);
    let errors = import_at_once(&f.data, &[first, second]);
    assert!(errors.is_empty(), "{errors:?}");

    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods.len(), 2);
    let archives = f.data.game(SLOT_GAME).archives();
    assert_eq!(fs::read_dir(&archives).unwrap().count(), 2);
}

#[test]
fn a_failure_mid_extract_leaves_no_library_change_and_the_next_import_succeeds() {
    for kind in [io::ErrorKind::StorageFull, io::ErrorKind::PermissionDenied] {
        let f = fixture(&format!("import-fault-{kind:?}"));
        let zip = f.zip(
            "fault.zip",
            &[
                ("Mods/modA/content/a.ws", "a"),
                ("Mods/modA/content/b.ws", "b"),
            ],
        );
        crate::import::fail_after(2, kind);
        let error = import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap_err();
        crate::import::fail_after(0, kind);
        assert!(!error.code.is_empty(), "{error:?}");

        // The working copy and the reserved archive are released by their Drop guards; the index was never saved.
        let game = f.data.game(SLOT_GAME);
        assert!(!game.archives().join("fault.zip").exists());
        let leftovers: Vec<_> = fs::read_dir(game.library()).map_or_else(
            |_| Vec::new(),
            |entries| entries.flatten().map(|e| e.file_name()).collect(),
        );
        assert!(
            leftovers
                .iter()
                .all(|name| !name.to_string_lossy().starts_with(".importing-")),
            "{leftovers:?}"
        );
        assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods.len(), 0);

        import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
        assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods.len(), 1);
    }
}

#[test]
fn a_failed_import_leaves_no_archive_or_working_folder_behind() {
    let f = fixture("failed");
    let zip = f.base.join("broken.zip");
    fs::write(&zip, "not an archive").unwrap();
    assert!(import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).is_err());

    let game = f.data.game(SLOT_GAME);
    assert!(!game.archives().join("broken.zip").exists());
    let leftovers: Vec<_> = fs::read_dir(game.library()).map_or_else(
        |_| Vec::new(),
        |entries| entries.flatten().map(|e| e.file_name()).collect(),
    );
    assert!(
        leftovers
            .iter()
            .all(|name| !name.to_string_lossy().starts_with(".importing-")),
        "{leftovers:?}"
    );
}

#[test]
fn reinstalling_with_other_fomod_options_replaces_the_files_and_remembers_the_answer() {
    let f = fixture("fomod-reinstall-options");
    let zip = f.zip(
        "installer.zip",
        &[
            (
                "fomod/ModuleConfig.xml",
                r#"<config>
                    <moduleName>Installer</moduleName>
                    <installSteps>
                        <installStep name="Files">
                            <optionalFileGroups>
                                <group name="Variants" type="SelectExactlyOne">
                                    <plugins>
                                        <plugin name="A">
                                            <files><file source="Mods/modA/content/a.ws" destination="Mods/modA/content/a.ws"/></files>
                                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                                        </plugin>
                                        <plugin name="B">
                                            <files><file source="Mods/modA/content/b.ws" destination="Mods/modA/content/b.ws"/></files>
                                            <typeDescriptor><type name="Optional"/></typeDescriptor>
                                        </plugin>
                                    </plugins>
                                </group>
                            </optionalFileGroups>
                        </installStep>
                    </installSteps>
                </config>"#,
            ),
            ("Mods/modA/content/a.ws", "a"),
            ("Mods/modA/content/b.ws", "b"),
        ],
    );
    let choice = |option: &str| crate::domain::fomod::Choice {
        step: 0,
        group_index: 0,
        group: "Variants".to_string(),
        option: option.to_string(),
    };
    import(
        &f.data,
        SLOT_GAME,
        &zip,
        Some(&[choice("A")]),
        &mut |_, _| {},
    )
    .unwrap();
    let imported = status(&f.data, SLOT_GAME).unwrap();
    let folder = imported.mods[0].folder.clone();

    let fomods = crate::import::reinstall_inspect(&f.data, SLOT_GAME, &folder).unwrap();
    assert!(matches!(
        fomods,
        crate::import::model::ZipInspect::Fomod { .. }
    ));

    crate::import::reinstall_mod_with(
        &f.data,
        SLOT_GAME,
        &folder,
        Some(&[choice("B")]),
        |data, _| data.ensure_idle(),
    )
    .unwrap();
    let library = f.data.game(SLOT_GAME).library().join(&folder);
    assert!(library.join("content/b.ws").is_file());
    assert!(!library.join("content/a.ws").exists());

    // The freshly unpacked folder must be re-scanned into the record, not just checked on disk: the index's file list
    // has to switch from the old option's file to the new option's file.
    let game_data = f.data.game(SLOT_GAME);
    let record = load_library(&game_data, &test_games::slot())
        .unwrap()
        .versions[&folder]
        .clone();
    assert_eq!(
        record.files.iter().map(String::as_str).collect::<Vec<_>>(),
        ["content/b.ws"]
    );

    let details = crate::status::details::mod_details(&f.data, SLOT_GAME, &folder).unwrap();
    assert_eq!(details.extras.fomod.len(), 1);
    assert_eq!(details.extras.fomod[0].option, "B");
    assert!(details.extras.fomod_installer);
}

#[test]
fn a_loose_import_can_be_canceled_before_it_starts() {
    let f = fixture("import-cancel");
    let zip = f.zip("mod.zip", &[("Mods/modA/content/x.ws", "x")]);
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let error = crate::import::import_archive(
        &f.data,
        SLOT_GAME,
        crate::import::ImportRequest::new(&zip),
        Some(&cancel),
        &mut |_, _| {},
    )
    .unwrap_err();
    assert_eq!(error.code, "import.cancelled");
    assert!(status(&f.data, SLOT_GAME).unwrap().mods.is_empty());
}

#[test]
fn a_batch_import_can_be_canceled_before_its_first_group() {
    let f = fixture("pack-cancel");
    let zip = f.zip("mod.zip", &[("Mods/modA/content/x.ws", "x")]);
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let error = crate::import::import_batch(
        &f.data,
        SLOT_GAME,
        vec![crate::import::ImportRequest::new(zip)],
        &mut |_, _| {},
        Some(&cancel),
    )
    .unwrap_err();
    assert_eq!(error.code, crate::import::IMPORT_CANCELLED);
}
