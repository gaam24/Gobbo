//! Other stores that keep a game folder: Epic's launcher list, Heroic's legendary/GOG stores, GOG's `goggame-*.info`
//! folders and Lutris's game files. Steam has its own library and manifest reader in `detect.rs`; this module only turns
//! the other stores into candidates, and `detect` checks the same markers as for Steam. A definition opts into a store
//! with an id (`epic_ids`, `gog_ids`, `heroic_slugs`, `lutris_slugs`), so a folder with the right files is not taken
//! for a store that never named it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;

use super::GameDef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Store {
    Gog,
    Epic,
    Heroic,
    Lutris,
}

impl Store {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gog => "gog",
            Self::Epic => "epic",
            Self::Heroic => "heroic",
            Self::Lutris => "lutris",
        }
    }
}

/// One folder a store says holds a game, before the markers are checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub store: Store,
    pub root: PathBuf,
    /// The store's id: an Epic `AppName`, a GOG product id, a Heroic app or a Lutris slug.
    pub id: String,
}

/// Every installed game the non-Steam stores name. A missing or unreadable store file simply adds nothing.
pub fn candidates(home: &Path, program_data: Option<&Path>) -> Vec<Candidate> {
    let mut out = Vec::new();
    epic(&mut out, program_data);
    heroic(&mut out, home);
    gog(&mut out, home, program_data);
    lutris(&mut out, home);
    out
}

/// Whether the candidate belongs to this game: the markers must be there, and the store id must be one the definition
/// opted into.
pub fn matches(game: &GameDef, candidate: &Candidate) -> bool {
    // The cheap id check first: a candidate the game does not opt into never needs a folder stat.
    let has = |ids: &[String]| ids.iter().any(|id| id.eq_ignore_ascii_case(&candidate.id));
    let opted_in = match candidate.store {
        Store::Gog => has(&game.detect.gog_ids),
        Store::Epic => has(&game.detect.epic_ids),
        Store::Heroic => {
            has(&game.detect.heroic_slugs)
                || has(&game.detect.epic_ids)
                || has(&game.detect.gog_ids)
        }
        Store::Lutris => has(&game.detect.lutris_slugs),
    };
    opted_in && super::detect::has_markers(&candidate.root, game)
}

fn epic(out: &mut Vec<Candidate>, program_data: Option<&Path>) {
    let Some(program_data) = program_data else {
        return;
    };
    let path = program_data.join("Epic/UnrealEngineLauncher/LauncherInstalled.dat");
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(list) = json
        .get("InstallationList")
        .and_then(|value| value.as_array())
    else {
        return;
    };

    for item in list {
        let (Some(id), Some(root)) = (
            item.get("AppName").and_then(|value| value.as_str()),
            item.get("InstallLocation").and_then(|value| value.as_str()),
        ) else {
            continue;
        };
        out.push(Candidate {
            store: Store::Epic,
            root: PathBuf::from(root),
            id: id.to_string(),
        });
    }
}

fn heroic(out: &mut Vec<Candidate>, home: &Path) {
    // Legendary keeps a map of AppName -> install; GOG keeps a list with `appName` and `install_path`.
    let legendary = home.join(".config/heroic/legendaryConfig/legendary/installed.json");

    if let Ok(text) = fs::read_to_string(&legendary)
        && let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&text)
    {
        for (id, value) in map {
            let Some(root) = value.get("install_path").and_then(|value| value.as_str()) else {
                continue;
            };
            out.push(Candidate {
                store: Store::Heroic,
                root: PathBuf::from(root),
                id,
            });
        }
    }

    let gog = home.join(".config/heroic/gog_store/installed.json");

    if let Ok(text) = fs::read_to_string(&gog)
        && let Ok(serde_json::Value::Array(list)) = serde_json::from_str::<serde_json::Value>(&text)
    {
        for item in list {
            let (Some(id), Some(root)) = (
                item.get("appName").and_then(|value| value.as_str()),
                item.get("install_path").and_then(|value| value.as_str()),
            ) else {
                continue;
            };
            out.push(Candidate {
                store: Store::Heroic,
                root: PathBuf::from(root),
                id: id.to_string(),
            });
        }
    }
}

fn gog(out: &mut Vec<Candidate>, home: &Path, program_data: Option<&Path>) {
    let mut roots = vec![home.join("GOG Games"), home.join("Games")];

    if let Some(program_data) = program_data {
        roots.push(program_data.join("GOG.com/Galaxy/Games"));
    }

    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let Ok(files) = fs::read_dir(&path) else {
                continue;
            };

            for file in files.flatten() {
                let name = file.file_name().to_string_lossy().into_owned();

                if let Some(id) = name
                    .strip_prefix("goggame-")
                    .and_then(|rest| rest.strip_suffix(".info"))
                {
                    out.push(Candidate {
                        store: Store::Gog,
                        root: path,
                        id: id.to_string(),
                    });
                    break;
                }
            }
        }
    }
}

fn lutris(out: &mut Vec<Candidate>, home: &Path) {
    let dir = home.join(".local/share/lutris/games");
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let is_yaml = path.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml")
        });

        if !is_yaml {
            continue;
        }

        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Some(slug) = value_of(&text, "game_slug") else {
            continue;
        };
        let Some(directory) = value_of(&text, "directory") else {
            continue;
        };
        out.push(Candidate {
            store: Store::Lutris,
            root: PathBuf::from(directory),
            id: slug,
        });
    }
}

/// A top-level `key: value` line of a Lutris file. Quotes around the value are dropped.
fn value_of(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (found, value) = line.trim().split_once(':')?;

        if found != key {
            return None;
        }

        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games as test_games;
    use crate::test_support::{scratch, write};

    fn install(root: &Path) {
        write(&root.join("bin/game.exe"), "");
        write(&root.join("content/base/x.txt"), "");
    }

    /// An Epic launcher list with the slot game under `SlotApp`, and the fixture's folder.
    fn epic_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf, GameDef) {
        let base = scratch!(name);
        let program_data = base.join("program-data");
        let root = base.join("Epic Games/Slot");
        install(&root);
        write(
            &program_data.join("Epic/UnrealEngineLauncher/LauncherInstalled.dat"),
            &serde_json::json!({
                "InstallationList": [{
                    "AppName": "SlotApp",
                    "InstallLocation": root.display().to_string(),
                }],
            })
            .to_string(),
        );
        let mut game = test_games::slot();
        game.detect.epic_ids = vec!["SlotApp".into()];
        (base, program_data, root, game)
    }

    #[test]
    fn epic_reads_the_launcher_list_and_needs_the_app_name() {
        let (base, program_data, _root, mut game) = epic_fixture("stores-epic");
        let candidates = candidates(&base.join("home"), Some(&program_data));
        assert_eq!(
            candidates
                .iter()
                .find(|candidate| matches(&game, candidate))
                .map(|candidate| candidate.store),
            Some(Store::Epic)
        );

        game.detect.epic_ids = vec!["OtherApp".into()];
        assert!(!candidates.iter().any(|candidate| matches(&game, candidate)));
    }

    #[test]
    fn heroic_reads_legendary_and_gog_manifests() {
        let base = scratch!("stores-heroic");
        let home = base.join("home");
        let root = base.join("Games/Heroic/Slot");
        install(&root);
        write(
            &home.join(".config/heroic/legendaryConfig/legendary/installed.json"),
            &serde_json::json!({
                "SlotApp": { "install_path": root.display().to_string() },
            })
            .to_string(),
        );
        let mut game = test_games::slot();
        game.detect.epic_ids = vec!["SlotApp".into()];
        let candidates = candidates(&home, None);
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.store == Store::Heroic && matches(&game, candidate))
        );
    }

    #[test]
    fn a_scan_uses_the_store_candidate_but_not_a_folder_whose_store_id_is_missing() {
        let (base, program_data, root, mut game) = epic_fixture("stores-scan");
        let scans = super::super::detect::scan_with(
            &base.join("home"),
            Some(&program_data),
            &[game.clone()],
        );
        assert_eq!(
            scans[0].found.as_ref().map(|found| found.store.as_str()),
            Some("epic")
        );
        assert_eq!(
            scans[0]
                .found
                .as_ref()
                .map(|found| found.game_root.as_str()),
            Some(root.display().to_string().as_str())
        );

        game.detect.epic_ids.clear();
        let scans =
            super::super::detect::scan_with(&base.join("home"), Some(&program_data), &[game]);
        assert!(scans[0].found.is_none());
    }

    #[test]
    fn gog_and_lutris_are_matched_by_their_opt_in_ids() {
        let base = scratch!("stores-gog-lutris");
        let home = base.join("home");
        let gog_root = home.join("GOG Games/Slot");
        install(&gog_root);
        write(&gog_root.join("goggame-123.info"), "{}");
        let lutris_root = base.join("Games/Lutris/Slot");
        install(&lutris_root);
        write(
            &home.join(".local/share/lutris/games/slot.yml"),
            &format!(
                "game_slug: slot-game\ndirectory: \"{}\"\n",
                lutris_root.display()
            ),
        );

        let mut game = test_games::slot();
        game.detect.gog_ids = vec!["123".into()];
        game.detect.lutris_slugs = vec!["slot-game".into()];
        let candidates = candidates(&home, None);
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.store == Store::Gog && matches(&game, candidate))
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate.store == Store::Lutris && matches(&game, candidate))
        );

        game.detect.gog_ids.clear();
        game.detect.lutris_slugs.clear();
        assert!(!candidates.iter().any(|candidate| matches(&game, candidate)));
    }
}
