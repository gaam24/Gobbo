//! The Nexus page of a mod: what a checked page writes into the record, its picture, its id, and the missing info.

use super::*;
use crate::providers::nexus;
use crate::status::sync_profiles as status;
use crate::test_support::import;

#[test]
fn a_checked_page_writes_author_category_and_time_into_the_record() {
    let mut record = Family {
        id: "local:moda".into(),
        name: "modA".into(),
        ..Family::default()
    };
    let page = nexus::ModPage {
        version: "2.0".into(),
        name: Some("Example Sort Mod".into()),
        author: Some("TestAuthor".into()),
        category: Some("Models & Textures".into()),
        summary: Some("Sorts it all".into()),
        picture_url: Some("https://staticdelivery.nexusmods.com/a.png".into()),
        ..nexus::ModPage::default()
    };

    apply_nexus_page(
        &mut record,
        42,
        &page,
        1_730_000_000,
        Some("nexus-5.png".into()),
    );
    assert_eq!(
        record
            .source("nexus")
            .and_then(|source| source.meta.as_ref())
            .and_then(|meta| meta.picture.as_deref()),
        Some("nexus-5.png")
    );
    assert_eq!(
        record
            .source("nexus")
            .and_then(|source| source.meta.as_ref())
            .and_then(|meta| meta.summary.as_deref()),
        Some("Sorts it all")
    );
    // A later check that could not save the picture keeps the copy of the same picture.
    apply_nexus_page(&mut record, 42, &page, 1_730_000_000, None);

    let meta = record
        .source("nexus")
        .and_then(|source| source.meta.as_ref())
        .expect("the page writes the metadata");
    assert_eq!(meta.name.as_deref(), Some("Example Sort Mod"));
    assert_eq!(meta.author.as_deref(), Some("TestAuthor"));
    assert_eq!(meta.category.as_deref(), Some("Models & Textures"));
    assert_eq!(meta.checked_at, Some(1_730_000_000));
    assert_eq!(meta.picture.as_deref(), Some("nexus-5.png"));
}

#[test]
fn a_read_without_summary_or_dates_keeps_the_ones_it_had() {
    let mut record = Family {
        id: "local:moda".into(),
        name: "modA".into(),
        ..Family::default()
    };
    let first = nexus::ModPage {
        version: "2.0".into(),
        summary: Some("Sorts it all".into()),
        updated_at: Some(1_700_000_000),
        created_at: Some(1_600_000_000),
        unique_downloads: Some(90),
        ..nexus::ModPage::default()
    };
    apply_nexus_page(&mut record, 42, &first, 1_730_000_000, None);

    // A later read whose GraphQL facets failed brings neither the summary nor the dates.
    let later = nexus::ModPage {
        version: "2.0".into(),
        unique_downloads: Some(95),
        ..nexus::ModPage::default()
    };
    apply_nexus_page(&mut record, 42, &later, 1_731_000_000, None);

    let meta = record
        .source("nexus")
        .and_then(|source| source.meta.as_ref())
        .expect("the page writes the metadata");
    assert_eq!(meta.summary.as_deref(), Some("Sorts it all"));
    assert_eq!(meta.updated_at, Some(1_700_000_000));
    assert_eq!(meta.created_at, Some(1_600_000_000));
    assert_eq!(meta.stats.unique_downloads, Some(95));
    assert_eq!(meta.checked_at, Some(1_731_000_000));
}

#[test]
fn setting_a_nexus_id_moves_the_family_and_merges_a_later_import() {
    let f = mirror_fixture("nexus-merge");
    let local = f.zip("Foo-1.zip", &[("r6/scripts/Foo/a.reds", "one")]);
    import(&f.data, MIRROR_GAME, &local, None, &mut |_, _| {}).unwrap();
    let before = status(&f.data, MIRROR_GAME).unwrap();
    let old_id = before.mods[0].id.clone();
    assert!(old_id.starts_with("local:"));

    set_mod_source(&f.data, MIRROR_GAME, &old_id, "nexus", Some("1234".into())).unwrap();
    let moved = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(moved.mods[0].id, "nexus:1234");
    assert_eq!(moved.mods[0].sources[0].provider, "nexus");
    assert_eq!(moved.mods[0].sources[0].mod_id, "1234");
    assert_eq!(
        moved.mods[0].sources[0].page_url.as_deref(),
        Some("https://www.nexusmods.com/mirror/mods/1234")
    );

    let nexus = f.zip(
        "Sorted Things 1234 2.0 2026-10-23T17-21Z abc.zip",
        &[("r6/scripts/Sorted/a.reds", "two")],
    );
    import(&f.data, MIRROR_GAME, &nexus, None, &mut |_, _| {}).unwrap();
    let after = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(after.mods.len(), 1);
    assert_eq!(after.mods[0].id, "nexus:1234");
    assert_eq!(after.mods[0].version_count, 2);
}

#[test]
fn a_saved_page_picture_comes_back_as_a_data_url() {
    let f = mirror_fixture("mod-picture");
    let images = f.data.game(MIRROR_GAME).dir.join("images");
    fs::create_dir_all(&images).unwrap();
    fs::write(images.join("nexus-107.png"), [0x89, b'P', b'N', b'G']).unwrap();
    let url = read_picture(&f.data.game(MIRROR_GAME).images(), "nexus-107.png")
        .unwrap()
        .unwrap();
    assert!(url.starts_with("data:image/png;base64,"), "{url}");
    assert_eq!(
        read_picture(&f.data.game(MIRROR_GAME).images(), "nexus-1.jpg").unwrap(),
        None
    );

    for bad in ["../library.json", "nexus-107.exe", ""] {
        assert_eq!(
            read_picture(&f.data.game(MIRROR_GAME).images(), bad)
                .unwrap_err()
                .code,
            "source.bad_picture"
        );
    }
}

#[test]
fn filling_nexus_info_asks_nothing_when_no_mod_needs_it() {
    let f = mirror_fixture("fill-nothing");
    let zip = f.zip("plain.zip", &[("bin/x64/plugins/x.asi", "x")]);
    import(&f.data, MIRROR_GAME, &zip, None, &mut |_, _| {}).unwrap();
    // No mod has a Nexus id, so no request goes out, whatever the key.
    assert_eq!(fill_source_info(&f.data, MIRROR_GAME).unwrap(), 0);
}
