//! Parity of the library record: the same archive gives the same name, folder, version and family whichever feature
//! imported it. These tests drive the real `acquire` path with the request shapes the production callers build.

use std::fs;
use std::path::Path;

use super::*;
use crate::acquire::{Acquired, BatchRequest, Choices, Origin, Request, acquire, acquire_batch};
use crate::domain::source::SourceRef;
use crate::infra::sidecar;
use crate::library::rescan;
use sha2::Sha256;

const PACKAGE_ARCHIVE: &str = "owner_name-package_name-2.0.0.zip";
const PACKAGE_ID: &str = "owner_name-package_name";

/// A Thunderstore-shaped archive whose `info.json` carries a stale version. The file name and the provider's answer
/// for the file are the two sources that must decide the record, not the author's `info.json`.
fn package_zip(dir: &Path) -> std::path::PathBuf {
    crate::test_support::make_zip(
        dir,
        PACKAGE_ARCHIVE,
        &[
            (
                "Mods/modPackage/info.json",
                r#"{"modName":"Package Name","version":"0.1"}"#,
            ),
            ("Mods/modPackage/content/x.ws", "x"),
        ],
    )
}

/// The four values that make up the imported mod record.
fn record(acquired: Acquired) -> (String, Option<String>, String, String) {
    let Acquired::Imported(view) = acquired else {
        panic!("the archive must be imported, not refused or left needing choices");
    };
    let mod_view = &view.mods[0];
    (
        mod_view.name.clone(),
        mod_view.version.clone(),
        mod_view.folder.clone(),
        mod_view.family.clone(),
    )
}

/// The same four values as the status view, for the rescan parity test.
fn status_record(f: &Fixture) -> (String, Option<String>, String, String) {
    let view = status(&f.data, SLOT_GAME).unwrap();
    let mod_view = &view.mods[0];
    (
        mod_view.info.name.clone(),
        mod_view.info.version.clone(),
        mod_view.folder.clone(),
        mod_view.id.clone(),
    )
}

#[test]
fn the_same_archive_imports_the_same_record_from_every_path() {
    let source = SourceRef::new("thunderstore", PACKAGE_ID).with_file("2.0.0");
    let mut records = Vec::new();

    // A picked file with the sidecar a provider download carries.
    {
        let f = fixture("parity-manual");
        let zip = package_zip(&f.base);
        sidecar::write_source(&zip, source.clone());
        let acquired = acquire(
            &f.data,
            SLOT_GAME,
            Request {
                origin: Origin::File(&zip),
                source: None,
                choices: Choices::Ask,
            },
            None,
            &mut |_, _| {},
        )
        .unwrap();
        records.push(record(acquired));
    }

    // A download the queue installs: its source comes from the written sidecar.
    {
        let f = fixture("parity-download");
        let downloads = f.data.game(SLOT_GAME).downloads();
        fs::create_dir_all(&downloads).unwrap();
        let zip = package_zip(&downloads);
        sidecar::write_source(&zip, source.clone());
        let acquired = acquire(
            &f.data,
            SLOT_GAME,
            Request {
                origin: Origin::Download(&zip),
                source: None,
                choices: Choices::Ask,
            },
            None,
            &mut |_, _| {},
        )
        .unwrap();
        records.push(record(acquired));
    }

    // An update: the file a fetch produced, with its source and the choices kept from the old version. A fetch needs
    // the network, so the test starts at the local file the fetch would have left for `acquire`.
    {
        let f = fixture("parity-update");
        let zip = package_zip(&f.base);
        let acquired = acquire(
            &f.data,
            SLOT_GAME,
            Request {
                origin: Origin::File(&zip),
                source: Some(source.clone()),
                choices: Choices::Stored(Vec::new()),
            },
            None,
            &mut |_, _| {},
        )
        .unwrap();
        records.push(record(acquired));
    }

    // A requirement that names only a page: no file id, so the label carries the version.
    {
        let f = fixture("parity-requirement");
        let zip = package_zip(&f.base);
        let acquired = acquire(
            &f.data,
            SLOT_GAME,
            Request {
                origin: Origin::File(&zip),
                source: Some(SourceRef::new("thunderstore", PACKAGE_ID)),
                choices: Choices::Ask,
            },
            None,
            &mut |_, _| {},
        )
        .unwrap();
        records.push(record(acquired));
    }

    // A pack with the archive embedded: the batch takes the already hashed staged copy.
    {
        let f = fixture("parity-pack");
        let zip = package_zip(&f.base);
        let digest = crate::infra::io::digest_file::<Sha256>(&zip, &mut |_| {}).unwrap();
        let results = acquire_batch(
            &f.data,
            SLOT_GAME,
            vec![BatchRequest {
                origin: Origin::File(&zip),
                source: Some(source),
                choices: Choices::Stored(Vec::new()),
                staged: Some(digest),
            }],
            None,
            &mut |_, _| {},
        )
        .unwrap();
        records.push(record(results.into_iter().next().unwrap().unwrap()));
    }

    for found in &records {
        assert_eq!(
            found, &records[0],
            "every import path must build the same record"
        );
    }

    assert_eq!(
        records[0],
        (
            "Package Name".to_string(),
            Some("2.0.0".to_string()),
            PACKAGE_ARCHIVE.trim_end_matches(".zip").to_string(),
            format!("thunderstore:{PACKAGE_ID}"),
        )
    );
}

#[test]
fn a_curator_name_does_not_rename_the_imported_mod() {
    let f = fixture("parity-curator-nexus");
    let archive = "Example Mod-7001-1-0-1584112564.zip";
    let curator = "A Totally Different Curator Name";
    let zip = f.zip(archive, &[("Mods/modExample/content/x.ws", "x")]);

    // The collection-shaped request carries no curator name: it cannot rename the mod or move its folder.
    let acquired = acquire(
        &f.data,
        SLOT_GAME,
        Request {
            origin: Origin::File(&zip),
            source: None,
            choices: Choices::Ask,
        },
        None,
        &mut |_, _| {},
    )
    .unwrap();
    let found = record(acquired);

    assert_eq!(found.0, "Example Mod", "the Nexus label names the mod");
    assert_eq!(
        found.2,
        archive.trim_end_matches(".zip"),
        "the folder keeps the archive file stem"
    );
    assert!(!found.0.contains(curator) && !found.2.contains(curator));
}

#[test]
fn the_provider_file_version_wins_over_the_archive_label() {
    let f = fixture("parity-provider-version");
    let zip = f.zip(
        "owner_name-package_name-1.0.0.zip",
        &[("Mods/modPackage/content/x.ws", "x")],
    );
    // The file name says 1.0.0, but the provider answers 2.0.0 for this exact file id.
    let source = SourceRef::new("thunderstore", "owner_name-package_name").with_file("2.0.0");
    let acquired = acquire(
        &f.data,
        SLOT_GAME,
        Request {
            origin: Origin::File(&zip),
            source: Some(source),
            choices: Choices::Ask,
        },
        None,
        &mut |_, _| {},
    )
    .unwrap();

    assert_eq!(record(acquired).1.as_deref(), Some("2.0.0"));
}

#[test]
fn a_rescan_keeps_the_name_and_version_the_import_decided() {
    let f = fixture("parity-rescan");
    let zip = package_zip(&f.base);
    let source = SourceRef::new("thunderstore", PACKAGE_ID).with_file("2.0.0");
    acquire(
        &f.data,
        SLOT_GAME,
        Request {
            origin: Origin::File(&zip),
            source: Some(source),
            choices: Choices::Ask,
        },
        None,
        &mut |_, _| {},
    )
    .unwrap();
    let before = status_record(&f);

    rescan(&f.data, SLOT_GAME).unwrap();

    assert_eq!(status_record(&f), before);
    assert_eq!(before.0, "Package Name");
    assert_eq!(before.1.as_deref(), Some("2.0.0"));
}
