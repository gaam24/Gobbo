//! The shared install path: an archive from a file, a download or a provider file ends in the library once, with the
//! same duplicate, installer and cancellation rules whichever feature brought it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::*;
use crate::acquire::{Acquired, BatchRequest, Choices, Origin, Request, acquire, acquire_batch};
use crate::domain::fomod::Choice;
use crate::domain::source::SourceRef;

/// A plain archive with one mod whose one file holds `text`, written into `dir`.
fn plain_zip(dir: &Path, name: &str, text: &str) -> PathBuf {
    crate::test_support::make_zip(dir, name, &[("Mods/modA/content/x.ws", text)])
}

/// An installer archive with one group that asks for one of two variants, `A` or `B`.
fn installer_zip(f: &Fixture, name: &str) -> PathBuf {
    f.zip(
        name,
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
    )
}

/// The choice of the variant `option` in the installer of `installer_zip`.
fn variant(option: &str) -> Choice {
    Choice {
        step: 0,
        group_index: 0,
        group: "Variants".to_string(),
        option: option.to_string(),
    }
}

/// The request for an archive with nothing else known.
fn request(origin: Origin<'_>, choices: Choices) -> Request<'_> {
    Request {
        origin,
        source: None,
        choices,
    }
}

#[test]
fn a_plain_archive_from_a_file_is_imported_and_the_file_is_kept() {
    let f = fixture("acquire-file");
    let zip = plain_zip(&f.base, "plain.zip", "from A");

    let acquired = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();

    assert!(matches!(acquired, Acquired::Imported(_)));
    assert!(zip.is_file(), "a file the user picked is never deleted");
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods.len(), 1);
}

#[test]
fn a_second_import_of_the_same_archive_names_the_first_folder() {
    let f = fixture("acquire-duplicate");
    let zip = plain_zip(&f.base, "plain.zip", "from A");
    let first = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();
    let Acquired::Imported(first) = first else {
        panic!("the first import must bring the archive in");
    };
    let folder = first.mods[0].folder.clone();

    let again = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();

    match again {
        Acquired::Existing { folder: existing } => assert_eq!(existing, folder),
        _ => panic!("a duplicate must answer with the folder that holds it"),
    }

    assert!(zip.is_file(), "a duplicate picked from a file is kept");
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods.len(), 1);
}

#[test]
fn a_duplicate_download_is_deleted_and_a_duplicate_file_is_kept() {
    let f = fixture("acquire-duplicate-download");
    let zip = plain_zip(&f.base, "plain.zip", "from A");
    acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();

    let downloads = f.data.game(SLOT_GAME).downloads();
    fs::create_dir_all(&downloads).unwrap();
    let copy = downloads.join("plain-copy.zip");
    fs::copy(&zip, &copy).unwrap();

    let from_download = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::Download(&copy), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();
    assert!(matches!(from_download, Acquired::Existing { .. }));
    assert!(
        !copy.exists(),
        "a duplicate download leaves the downloads folder"
    );

    let kept = downloads.join("plain-kept.zip");
    fs::copy(&zip, &kept).unwrap();
    let as_file = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&kept), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();
    assert!(matches!(as_file, Acquired::Existing { .. }));
    assert!(kept.is_file(), "a duplicate picked as a file is kept");
}

#[test]
fn a_provider_file_already_in_the_library_is_not_imported_again() {
    let f = fixture("acquire-known-file");
    let first = plain_zip(&f.base, "first.zip", "from A");
    let source = SourceRef::new("nexus", "1").with_file("2");
    let mut first_request = request(Origin::File(&first), Choices::Ask);
    first_request.source = Some(source.clone());
    acquire(&f.data, SLOT_GAME, first_request, None, &mut |_, _| {}).unwrap();

    // A different archive, carrying the same provider file: the library already has that file.
    let second = plain_zip(&f.base, "second.zip", "from B");
    let mut second_request = request(Origin::File(&second), Choices::Ask);
    second_request.source = Some(source);
    let acquired = acquire(&f.data, SLOT_GAME, second_request, None, &mut |_, _| {}).unwrap();

    assert!(matches!(acquired, Acquired::Existing { .. }));
    let mods = status(&f.data, SLOT_GAME).unwrap().mods;
    assert_eq!(mods.len(), 1, "the second archive was not imported");
}

#[test]
fn an_installer_without_choices_asks_for_them_and_imports_nothing() {
    let f = fixture("acquire-fomod-ask");
    let zip = installer_zip(&f, "installer.zip");

    let acquired = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();

    match acquired {
        Acquired::NeedsChoice(needed) => assert_eq!(needed.path, zip),
        _ => panic!("an installer with no choices must ask for them"),
    }

    assert!(status(&f.data, SLOT_GAME).unwrap().mods.is_empty());
}

#[test]
fn an_installer_download_that_asks_for_choices_is_kept_for_the_wizard() {
    let f = fixture("acquire-fomod-download");
    let downloads = f.data.game(SLOT_GAME).downloads();
    fs::create_dir_all(&downloads).unwrap();
    let source = installer_zip(&f, "installer.zip");
    let copy = downloads.join("installer.zip");
    fs::copy(&source, &copy).unwrap();

    let acquired = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::Download(&copy), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();

    match acquired {
        Acquired::NeedsChoice(needed) => assert_eq!(needed.path, copy),
        _ => panic!("an installer with no choices must ask for them"),
    }

    assert!(
        copy.is_file(),
        "an installer download that needs choices stays for the wizard"
    );
    assert!(status(&f.data, SLOT_GAME).unwrap().mods.is_empty());
}

#[test]
fn an_installer_with_stored_choices_is_imported_with_them() {
    let f = fixture("acquire-fomod-stored");
    let zip = installer_zip(&f, "installer.zip");

    let acquired = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Stored(vec![variant("A")])),
        None,
        &mut |_, _| {},
    )
    .unwrap();

    assert!(matches!(acquired, Acquired::Imported(_)));
    let imported = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(imported.mods.len(), 1);
    let folder = f
        .data
        .game(SLOT_GAME)
        .library()
        .join(&imported.mods[0].folder);
    assert!(folder.join("content/a.ws").is_file());
    assert!(!folder.join("content/b.ws").exists());
}

#[test]
fn an_installer_with_given_choices_is_imported_with_them() {
    let f = fixture("acquire-fomod-given");
    let zip = installer_zip(&f, "installer.zip");

    let acquired = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Given(vec![variant("B")])),
        None,
        &mut |_, _| {},
    )
    .unwrap();

    assert!(matches!(acquired, Acquired::Imported(_)));
    let imported = status(&f.data, SLOT_GAME).unwrap();
    let folder = f
        .data
        .game(SLOT_GAME)
        .library()
        .join(&imported.mods[0].folder);
    assert!(folder.join("content/b.ws").is_file());
    assert!(!folder.join("content/a.ws").exists());
}

#[test]
fn a_stopped_download_is_deleted_and_the_stop_is_reported_as_cancelled() {
    let f = fixture("acquire-cancelled");
    let downloads = f.data.game(SLOT_GAME).downloads();
    fs::create_dir_all(&downloads).unwrap();
    let copy = plain_zip(&downloads, "stopped.zip", "from A");
    let cancel = AtomicBool::new(true);

    let result = acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::Download(&copy), Choices::Ask),
        Some(&cancel),
        &mut |_, _| {},
    );
    let Err(error) = result else {
        panic!("a stopped import must not succeed");
    };

    assert!(error.is_cancelled(), "{error}");
    assert!(
        !copy.exists(),
        "a stopped download leaves the downloads folder"
    );
    assert!(status(&f.data, SLOT_GAME).unwrap().mods.is_empty());
}

#[test]
fn a_stopped_batch_deletes_its_fetched_downloads_and_reports_the_cancel() {
    let f = fixture("acquire-batch-cancelled");
    let downloads = f.data.game(SLOT_GAME).downloads();
    fs::create_dir_all(&downloads).unwrap();
    let first = plain_zip(&downloads, "first.zip", "from A");
    let second = plain_zip(&downloads, "second.zip", "from B");
    let cancel = AtomicBool::new(true);

    let result = acquire_batch(
        &f.data,
        SLOT_GAME,
        vec![
            BatchRequest {
                origin: Origin::Download(&first),
                source: None,
                choices: Choices::Ask,
                staged: None,
            },
            BatchRequest {
                origin: Origin::Download(&second),
                source: None,
                choices: Choices::Ask,
                staged: None,
            },
        ],
        Some(&cancel),
        &mut |_, _| {},
    );
    let Err(error) = result else {
        panic!("a stopped batch must not succeed");
    };

    assert!(error.is_cancelled(), "{error}");
    assert!(
        !first.exists() && !second.exists(),
        "every archive a stopped batch fetched leaves the downloads folder"
    );
    assert!(status(&f.data, SLOT_GAME).unwrap().mods.is_empty());
}

#[test]
fn a_duplicate_of_a_batch_download_is_deleted() {
    let f = fixture("acquire-batch-duplicate");
    let zip = plain_zip(&f.base, "plain.zip", "from A");
    acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();

    let downloads = f.data.game(SLOT_GAME).downloads();
    fs::create_dir_all(&downloads).unwrap();
    let copy = downloads.join("plain-copy.zip");
    fs::copy(&zip, &copy).unwrap();

    let results = acquire_batch(
        &f.data,
        SLOT_GAME,
        vec![BatchRequest {
            origin: Origin::Download(&copy),
            source: None,
            choices: Choices::Ask,
            staged: None,
        }],
        None,
        &mut |_, _| {},
    )
    .unwrap();

    assert!(matches!(
        results.as_slice(),
        [Ok(Acquired::Existing { .. })]
    ));
    assert!(
        !copy.exists(),
        "a duplicate download leaves the downloads folder through the batch path too"
    );
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods.len(), 1);
}

#[test]
fn a_reinstall_keeps_the_saved_fomod_answers_unless_new_ones_are_given() {
    let f = fixture("acquire-reinstall-choices");
    let zip = installer_zip(&f, "installer.zip");
    acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Given(vec![variant("A")])),
        None,
        &mut |_, _| {},
    )
    .unwrap();
    let folder = status(&f.data, SLOT_GAME).unwrap().mods[0].folder.clone();
    let library = f.data.game(SLOT_GAME).library().join(&folder);
    assert!(library.join("content/a.ws").is_file());
    assert!(!library.join("content/b.ws").exists());

    // No new answers: the ones the version was installed with are kept.
    crate::import::reinstall_mod(&f.data, SLOT_GAME, &folder).unwrap();
    assert!(library.join("content/a.ws").is_file());
    assert!(!library.join("content/b.ws").exists());
    let details = crate::status::details::mod_details(&f.data, SLOT_GAME, &folder).unwrap();
    assert_eq!(details.extras.fomod[0].option, "A");

    // New answers replace the saved ones.
    crate::import::reinstall_mod_with(
        &f.data,
        SLOT_GAME,
        &folder,
        Some(&[variant("B")]),
        |data, _| data.ensure_idle(),
    )
    .unwrap();
    assert!(library.join("content/b.ws").is_file());
    assert!(!library.join("content/a.ws").exists());
    let details = crate::status::details::mod_details(&f.data, SLOT_GAME, &folder).unwrap();
    assert_eq!(details.extras.fomod[0].option, "B");
}

#[test]
fn a_pack_download_of_a_file_already_in_the_library_is_not_a_failure() {
    use crate::import::ImportRequest;
    use crate::packs::download::import_downloaded;

    let f = fixture("acquire-pack-duplicate");
    let zip = plain_zip(&f.base, "plain.zip", "from A");
    acquire(
        &f.data,
        SLOT_GAME,
        request(Origin::File(&zip), Choices::Ask),
        None,
        &mut |_, _| {},
    )
    .unwrap();

    let downloads = f.data.game(SLOT_GAME).downloads();
    fs::create_dir_all(&downloads).unwrap();
    let copy = downloads.join("plain-copy.zip");
    fs::copy(&zip, &copy).unwrap();

    let failed = import_downloaded(
        &f.data,
        SLOT_GAME,
        vec![("Plain".to_string(), ImportRequest::new(copy.clone()))],
        None,
    )
    .unwrap();

    assert!(failed.is_empty(), "{failed:?}");
    assert!(!copy.exists(), "the duplicate download is deleted");
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods.len(), 1);
}

/// `Origin::Fetch` resolves its provider through the global registry, so it could not be tested without the network.
/// With the test-only seam a network-free provider answers: the archive it writes is imported with the mod name from
/// the file-name label, the version the provider gives for the exact file beating that label, and the fetched archive
/// is cleaned up after the import.
#[test]
fn a_fetched_provider_file_is_imported_with_the_providers_version() {
    let f = fixture("acquire-fetch");
    let source = SourceRef::new("nexus", "1001").with_file("2002");
    let _provider =
        crate::providers::testing::install_fake(crate::providers::testing::FakeConfig {
            id: "nexus",
            ..Default::default()
        });

    let acquired = acquire(
        &f.data,
        SLOT_GAME,
        Request {
            origin: Origin::Fetch { archive_name: None },
            source: Some(source),
            choices: Choices::Ask,
        },
        None,
        &mut |_, _| {},
    )
    .unwrap();

    let Acquired::Imported(view) = acquired else {
        panic!("a fetched archive must be imported, not refused or left needing choices");
    };
    let imported = &view.mods[0];
    assert_eq!(imported.name, "Example Provider File");
    assert_eq!(imported.version.as_deref(), Some("2.0.0"));
    assert_eq!(imported.folder, "Example Provider File-1001-1.0.0");
    assert_eq!(imported.family, "nexus:1001");

    let status = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(status.mods.len(), 1);
    assert_eq!(
        status.mods[0]
            .sources
            .iter()
            .find(|source| source.provider == "nexus")
            .map(|source| (source.mod_id.as_str(), source.file_id.as_deref())),
        Some(("1001", Some("2002"))),
        "the provider file reached the library with its mod and file id"
    );

    let downloads = f.data.game(SLOT_GAME).downloads();
    assert!(
        !downloads
            .join("Example Provider File-1001-1.0.0.zip")
            .exists(),
        "the fetched archive is cleaned up once it is imported"
    );
    assert!(
        !downloads
            .join("Example Provider File-1001-1.0.0.zip.source.json")
            .exists(),
        "the sidecar goes with the fetched archive"
    );
}
