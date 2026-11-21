//! Lutris keeps its games in a SQLite database, `<data>/pga.db`: table `games`, with the install folder in `directory`
//! (the YAML files do not hold it). Each game also has a YAML file, `<games>/<configpath>.yml`, whose `game:` section
//! names the Wine prefix. `<games>` is `<config>/games` while Lutris's old config folder exists, else `<data>/games`,
//! the rule Lutris itself uses. A game Lutris installed from GOG or the Epic store also carries that store's id.
//!
//! The database is opened read-only, so Gobbo never changes Lutris's data, even while Lutris runs.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::{Candidate, IdKind, Store};
use crate::platform::LutrisFolders;

/// One installed game as `pga.db` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    slug: String,
    directory: String,
    configpath: Option<String>,
    service: Option<String>,
    service_id: Option<String>,
}

/// Reads the installed games of one Lutris install. A missing or unreadable database adds nothing.
pub fn read(out: &mut Vec<Candidate>, folders: &LutrisFolders) {
    let games = if folders.config.is_dir() {
        folders.config.join("games")
    } else {
        folders.data.join("games")
    };

    for row in rows(&folders.data.join("pga.db")) {
        let root = PathBuf::from(row.directory.trim());
        let prefix = row
            .configpath
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .and_then(|name| fs::read_to_string(games.join(format!("{}.yml", name.trim()))).ok())
            .and_then(|text| section_value(&text, "game", "prefix"))
            .map(PathBuf::from);
        let candidate = |id: &str, kind| Candidate {
            store: Store::Lutris,
            root: root.clone(),
            id: id.to_string(),
            kind,
            prefix: prefix.clone(),
        };

        out.push(candidate(&row.slug, IdKind::Lutris));

        let kind = match row.service.as_deref() {
            Some("gog") => Some(IdKind::Gog),
            Some("egs") => Some(IdKind::Epic),
            _ => None,
        };

        if let (Some(kind), Some(id)) = (kind, row.service_id.as_deref())
            && !id.trim().is_empty()
        {
            out.push(candidate(id.trim(), kind));
        }
    }
}

/// The installed games of a database that has an install folder, by slug.
fn rows(database: &Path) -> Vec<Row> {
    if !database.is_file() {
        return Vec::new();
    }

    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(connection) = Connection::open_with_flags(database, flags) else {
        return Vec::new();
    };
    let Ok(mut query) = connection.prepare(
        "SELECT slug, directory, configpath, service, service_id FROM games \
         WHERE installed = 1 AND slug IS NOT NULL AND slug != '' \
         AND directory IS NOT NULL AND directory != '' ORDER BY slug",
    ) else {
        return Vec::new();
    };
    let Ok(found) = query.query_map([], |row| {
        Ok(Row {
            slug: row.get(0)?,
            directory: row.get(1)?,
            configpath: row.get(2)?,
            service: row.get(3)?,
            service_id: row.get(4)?,
        })
    }) else {
        return Vec::new();
    };

    found.flatten().collect()
}

/// A `key: value` line inside a top-level `section:` block, that is an indented line before the next top-level one.
/// Quotes around the value are dropped, and an empty value is no value.
fn section_value(text: &str, section: &str, key: &str) -> Option<String> {
    let header = format!("{section}:");
    let mut inside = false;

    text.lines().find_map(|line| {
        if !line.starts_with([' ', '\t']) {
            inside = line.trim_end() == header;
            return None;
        }

        if !inside {
            return None;
        }

        let (found, value) = line.trim().split_once(':')?;
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        (found == key && !value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{scratch, write};

    /// A Lutris database with the columns detection reads, and the given installs.
    fn database(path: &Path, games: &[(&str, &str, i64, &str, &str, &str)]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE games (id INTEGER PRIMARY KEY, name TEXT, slug TEXT, runner TEXT, \
                 directory TEXT, installed INTEGER, configpath TEXT, service TEXT, service_id TEXT);",
            )
            .unwrap();

        for (slug, directory, installed, configpath, service, service_id) in games {
            connection
                .execute(
                    "INSERT INTO games (name, slug, runner, directory, installed, configpath, service, service_id) \
                     VALUES (?1, ?1, 'wine', ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![slug, directory, installed, configpath, service, service_id],
                )
                .unwrap();
        }
    }

    #[test]
    fn the_database_names_each_installed_game_and_its_folder() {
        let base = scratch!("lutris-db");
        let folders = LutrisFolders {
            config: base.join("config/lutris"),
            data: base.join("data/lutris"),
        };
        database(
            &folders.data.join("pga.db"),
            &[
                (
                    "slot-game",
                    "/games/Lutris/Slot",
                    1,
                    "slot-game-1700000000",
                    "",
                    "",
                ),
                (
                    "removed-game",
                    "/games/Lutris/Removed",
                    0,
                    "removed-game-1",
                    "",
                    "",
                ),
                ("no-folder", "", 1, "no-folder-1", "", ""),
            ],
        );

        let mut out = Vec::new();
        read(&mut out, &folders);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].store, Store::Lutris);
        assert_eq!(out[0].kind, IdKind::Lutris);
        assert_eq!(out[0].id, "slot-game");
        assert_eq!(out[0].root, Path::new("/games/Lutris/Slot"));
        assert_eq!(out[0].prefix, None);
    }

    #[test]
    fn a_game_from_gog_or_epic_also_carries_that_store_id() {
        let base = scratch!("lutris-service");
        let folders = LutrisFolders {
            config: base.join("config/lutris"),
            data: base.join("data/lutris"),
        };
        database(
            &folders.data.join("pga.db"),
            &[
                (
                    "gog-game",
                    "/games/Lutris/Gog",
                    1,
                    "gog-game-1",
                    "gog",
                    "1000",
                ),
                (
                    "epic-game",
                    "/games/Lutris/Epic",
                    1,
                    "epic-game-1",
                    "egs",
                    "EpicApp",
                ),
            ],
        );

        let mut out = Vec::new();
        read(&mut out, &folders);
        let found: Vec<_> = out
            .iter()
            .map(|candidate| (candidate.id.as_str(), candidate.kind))
            .collect();
        assert_eq!(
            found,
            [
                ("epic-game", IdKind::Lutris),
                ("EpicApp", IdKind::Epic),
                ("gog-game", IdKind::Lutris),
                ("1000", IdKind::Gog),
            ]
        );
        assert!(out.iter().all(|candidate| candidate.store == Store::Lutris));
    }

    #[test]
    fn the_prefix_comes_from_the_game_file_in_the_folder_lutris_uses() {
        let base = scratch!("lutris-prefix");
        let folders = LutrisFolders {
            config: base.join("config/lutris"),
            data: base.join("data/lutris"),
        };
        database(
            &folders.data.join("pga.db"),
            &[("slot-game", "/games/Lutris/Slot", 1, "slot-game-1", "", "")],
        );
        let yaml = concat!(
            "game:\n",
            "  exe: /games/Lutris/Slot/Game.exe\n",
            "  prefix: \"/prefixes/Slot\"\n",
            "wine:\n",
            "  prefix: /not/this/one\n",
        );
        let prefix = |out: &[Candidate]| out[0].prefix.clone();

        // Without the old config folder, Lutris keeps the game files in its data folder.
        write(&folders.data.join("games/slot-game-1.yml"), yaml);
        let mut out = Vec::new();
        read(&mut out, &folders);
        assert_eq!(prefix(&out), Some(PathBuf::from("/prefixes/Slot")));

        // Once the config folder exists, the game files there win.
        write(
            &folders.config.join("games/slot-game-1.yml"),
            "game:\n  prefix: /prefixes/Config\n",
        );
        let mut out = Vec::new();
        read(&mut out, &folders);
        assert_eq!(prefix(&out), Some(PathBuf::from("/prefixes/Config")));
    }

    #[test]
    fn a_missing_or_foreign_database_names_nothing() {
        let base = scratch!("lutris-missing");
        let folders = LutrisFolders {
            config: base.join("config/lutris"),
            data: base.join("data/lutris"),
        };
        let mut out = Vec::new();
        read(&mut out, &folders);
        assert!(out.is_empty());

        write(&folders.data.join("pga.db"), "not a database");
        read(&mut out, &folders);
        assert!(out.is_empty());
    }

    #[test]
    fn a_section_value_ignores_other_sections_and_empty_values() {
        let text = "game:\n  prefix: ''\n  exe: a\nsystem:\n  prefix: /x\n";
        assert_eq!(section_value(text, "game", "prefix"), None);
        assert_eq!(section_value(text, "game", "exe"), Some("a".into()));
        assert_eq!(section_value(text, "system", "prefix"), Some("/x".into()));
        assert_eq!(section_value(text, "wine", "prefix"), None);
    }
}
