//! Library tests: the index, families, versions and the folders the library reads.

mod archives;
mod families;
mod info;
mod scan;

use crate::context::library::INDEX_FILE;
use std::path::PathBuf;

use std::fs;
use std::path::Path;

use super::*;
use crate::domain::profiles::entry::ProfileMod;
use crate::games::GameDef;
use crate::providers::nexus::NexusMeta;
use crate::test_support::{profile_mod, scratch, write};

fn game() -> GameDef {
    crate::test_support::games::slot()
}

fn scratch(name: &str) -> PathBuf {
    let base = scratch!(name);
    fs::create_dir_all(base.join("library")).unwrap();
    base
}

fn listed(folder: &str, enabled: bool) -> ProfileMod {
    profile_mod(folder, "Mods", enabled)
}

/// Writes one file into each mod folder of `mods`, and loads the library of `base` from a new index.
fn load_mods(base: &Path, mods: &[&str]) -> Library {
    let library_dir = base.join("library");
    for folder in mods {
        write(&library_dir.join(format!("{folder}/content/x.ws")), folder);
    }
    Library::load(&base.join(INDEX_FILE), &library_dir, &game()).unwrap()
}

/// Links modA to the archive `sha-a`, which the library keeps as `A-1.zip`.
fn link_archive_a(library: &mut Library) {
    library.versions.get_mut("modA").unwrap().archive = Some("sha-a".to_string());
    library.archives.insert(
        "sha-a".to_string(),
        Archive {
            original_name: "A-1.zip".to_string(),
            file: "A-1.zip".to_string(),
            source_files: std::collections::BTreeMap::new(),
            source_parts: std::collections::BTreeMap::new(),
            named: None,
            skipped: Vec::new(),
            skipped_total: 0,
        },
    );
}
