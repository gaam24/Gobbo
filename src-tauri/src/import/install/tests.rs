//! Import tests: archive layouts, folder names and the mod shapes the games need.

mod aliases;
mod formats;
mod full_root;
mod layout;
mod mirror;
mod nexus;
mod refusals;
mod shapes;
mod unreal;

use super::*;

use crate::test_support::{games as test_games, installed, make_zip};

const DIGEST: &str = "0123456789abcdef";

fn slot_game() -> GameDef {
    test_games::slot()
}

fn mirror_game() -> GameDef {
    test_games::mirror()
}

fn external_game() -> GameDef {
    test_games::external()
}

fn plugin_game() -> GameDef {
    test_games::plugin()
}

fn flat_game() -> GameDef {
    test_games::flat()
}

/// Imports the archive under its own name into `library`, with no choices and no progress.
fn install_into(zip: &Path, library: &Path, game: &GameDef, digest: &str) -> Import {
    install_zip(zip, library, game, digest, None, &mut |_| {}).unwrap()
}

/// Imports with the game's own top-level folders, so a wrapper around `bin/` or `r6/` is stripped.
fn install_into_with_roots(
    zip: &Path,
    library: &Path,
    game: &GameDef,
    roots: &[String],
    digest: &str,
) -> Import {
    let base_name = zip
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("mod")
        .to_string();
    install_zip_named_with_roots(
        zip,
        library,
        game,
        Naming {
            base_name: &base_name,
            digest,
            only: None,
        },
        None,
        None,
        roots,
        &mut |_| {},
    )
    .unwrap()
}

/// Compares the layout of imported mods. The checksum is asserted in the integrity tests; these shapes only care
/// about folders, slots, paths and sizes, so the digest is left out of the comparison.
fn assert_mods(actual: Vec<Installed>, expected: Vec<Installed>) {
    let actual: Vec<Installed> = actual
        .into_iter()
        .map(|mod_installed| Installed {
            integrity: None,
            ..mod_installed
        })
        .collect();
    assert_eq!(actual, expected);
}

/// Asserts that the import is one mod in the Mods slot, with the folder and the file count.
fn assert_one_mod(import: &Import, folder: &str, files: usize) {
    assert_eq!(import.mods.len(), 1);
    assert_eq!(import.mods[0].folder, folder);
    assert_eq!(import.mods[0].slot, "Mods");
    assert_eq!(import.mods[0].files, files);
}
