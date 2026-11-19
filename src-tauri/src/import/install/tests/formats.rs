//! Archive formats: a 7z archive installs like a zip.

use std::fs::File;

use super::*;
use crate::test_support::scratch;

#[test]
fn a_7z_archive_installs_like_a_zip() {
    let base = scratch!("7z-install");
    let library = base.join("library");
    let source = base.join("new.ws");
    fs::write(&source, "new").unwrap();
    let path = base.join("modA.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
    let entry =
        sevenz_rust2::ArchiveEntry::from_path(&source, "Mods/modA/content/new.ws".to_string());
    writer
        .push_archive_entry(entry, Some(File::open(&source).unwrap()))
        .unwrap();
    writer.finish().unwrap();

    let import = install_zip(&path, &library, &slot_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_eq!(import.mods[0].folder, "modA");
    assert_eq!(
        fs::read_to_string(library.join("modA/content/new.ws")).unwrap(),
        "new"
    );
}
