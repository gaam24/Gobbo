//! The definition cache: one shared value per unchanged `game.toml`, and a fresh parse after an edit. The tests use
//! their own cache, because another test may load the process-wide entries at the same time.

use super::*;
use crate::infra::file_cache::FileCache;
use crate::test_support::games::{SLOT, install};
use crate::test_support::{scratch, write};
use std::sync::Arc;

/// Writes the slot fixture under a new id in a scratch folder, and returns its `game.toml` and text.
fn probe(name: &str, id: &str) -> (PathBuf, String, FileCache<GameDef>) {
    install();
    let path = scratch!(name).join("game.toml");
    let text = SLOT.replace("id = \"slot\"", &format!("id = \"{id}\""));
    write(&path, &text);
    (path, text, FileCache::default())
}

#[test]
fn two_reads_of_one_definition_share_the_parsed_value() {
    let (path, _text, cache) = probe("definition-cache-shared", "cache-shared");

    let first = read_through(&cache, &path, loaders_dir()).expect("the definition loads");
    let second = read_through(&cache, &path, loaders_dir()).expect("a second read loads");
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(first.id, "cache-shared");
}

#[test]
fn an_edited_definition_is_parsed_again_and_a_bad_one_is_not_kept() {
    let (path, text, cache) = probe("definition-cache-reload", "cache-reload");
    let first = read_through(&cache, &path, loaders_dir()).expect("the definition loads");

    // A different length is a different cache key even when the clock has not moved.
    let renamed = text.replace("Slot Game", "Slot Game Renamed");
    assert_ne!(renamed.len(), text.len());
    write(&path, &renamed);
    let second = read_through(&cache, &path, loaders_dir()).expect("the edited definition loads");
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(second.name, "Slot Game Renamed");

    write(&path, "schema = 1\nid = \"cache-reload\"\n");
    for _ in 0..2 {
        let error =
            read_through(&cache, &path, loaders_dir()).expect_err("the bad definition is refused");
        assert_eq!(error.code, "games.invalid_definition");
    }

    write(&path, &renamed);
    assert_eq!(
        read_through(&cache, &path, loaders_dir())
            .expect("the corrected definition loads")
            .name,
        "Slot Game Renamed"
    );
}
