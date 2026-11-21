//! GOG names its installs in two ways. Every game folder holds one `goggame-<product id>.info` file per installed
//! product: the game and each installed DLC, side by side, as Heroic and Lutris read them. Galaxy and GOG's installers
//! also leave one registry subkey per product with its folder (read by `platform` on Windows). minigalaxy installs into
//! a folder of its own, named in its `config.json`.

use std::fs;
use std::path::{Path, PathBuf};

use super::{Candidate, IdKind, Store};

/// Reads the game folders one level under each library. A folder becomes one candidate per product it holds, so the
/// game matches by its own id whichever DLC files sit next to it.
pub fn read(out: &mut Vec<Candidate>, libraries: &[PathBuf]) {
    for library in libraries {
        let Ok(entries) = fs::read_dir(library) else {
            continue;
        };
        let mut folders: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        folders.sort();

        for folder in folders {
            for id in product_ids(&folder) {
                out.push(candidate(id, folder.clone()));
            }
        }
    }
}

/// The GOG product ids a folder's `goggame-<id>.info` files name, sorted. An empty list means no GOG install.
pub fn product_ids(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let id = name
                .to_str()?
                .strip_prefix("goggame-")?
                .strip_suffix(".info")?;
            (!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())).then(|| id.to_string())
        })
        .collect();
    ids.sort();
    ids
}

/// The candidates of the registry's `(product id, folder)` pairs.
pub fn registry(pairs: &[(String, PathBuf)]) -> Vec<Candidate> {
    pairs
        .iter()
        .map(|(id, folder)| candidate(id.clone(), folder.clone()))
        .collect()
}

/// The folder minigalaxy installs into: `install_dir` in its `config.json`. Its default, `~/GOG Games`, is a library
/// already.
pub fn minigalaxy_library(config: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(config).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    let folder = value.get("install_dir")?.as_str()?.trim();
    (!folder.is_empty()).then(|| PathBuf::from(folder))
}

/// A GOG candidate.
const fn candidate(id: String, root: PathBuf) -> Candidate {
    Candidate {
        store: Store::Gog,
        root,
        id,
        kind: IdKind::Gog,
        prefix: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{scratch, write};

    #[test]
    fn a_folder_names_every_product_it_holds_so_dlc_files_do_not_hide_the_game() {
        let base = scratch!("gog-folders");
        let library = base.join("GOG Games");
        let root = library.join("Slot");
        // The DLC's file sorts first; the game must still be a candidate under its own id.
        write(&root.join("goggame-1000.info"), "{}");
        write(&root.join("goggame-2000.info"), "{}");
        write(&root.join("goggame-2000.id"), "{}");
        write(&root.join("goggame-notes.info"), "{}");

        let mut out = Vec::new();
        read(&mut out, &[library]);
        let ids: Vec<_> = out.iter().map(|candidate| candidate.id.as_str()).collect();
        assert_eq!(ids, ["1000", "2000"]);
        assert!(out.iter().all(|candidate| candidate.root == root));
        assert!(out.iter().all(|candidate| candidate.kind == IdKind::Gog));
    }

    #[test]
    fn a_folder_without_a_gog_info_file_is_not_a_candidate() {
        let base = scratch!("gog-no-info");
        let library = base.join("GOG Games");
        write(&library.join("Slot/bin/game.exe"), "");

        let mut out = Vec::new();
        read(&mut out, &[library, base.join("missing")]);
        assert!(out.is_empty());
        assert!(product_ids(&base.join("missing")).is_empty());
    }

    #[test]
    fn registry_pairs_become_gog_candidates() {
        let pairs = [
            ("123".to_string(), PathBuf::from("C:\\Games\\Slot")),
            ("789".to_string(), PathBuf::from("/games/Other")),
        ];
        let candidates = registry(&pairs);
        assert_eq!(candidates.len(), 2);
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.store == Store::Gog)
        );
        assert_eq!(candidates[0].id, "123");
        assert_eq!(candidates[0].root, Path::new("C:\\Games\\Slot"));
    }

    #[test]
    fn minigalaxy_names_its_install_folder_and_a_broken_config_names_none() {
        let base = scratch!("gog-minigalaxy");
        let config = base.join("config.json");
        write(&config, r#"{"install_dir": "/games/Mini", "lang": "en"}"#);
        assert_eq!(
            minigalaxy_library(&config),
            Some(PathBuf::from("/games/Mini"))
        );

        write(&config, r#"{"install_dir": ""}"#);
        assert_eq!(minigalaxy_library(&config), None);
        write(&config, "not json");
        assert_eq!(minigalaxy_library(&config), None);
        assert_eq!(minigalaxy_library(&base.join("missing.json")), None);
    }
}
