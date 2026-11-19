//! Importing the mod list of a Mod Organizer 2 profile, its `modlist.txt`. The first line is the mod that wins, and the first
//! row of Gobbo's list wins too, so the order carries over as it is.

use crate::context::Data;
use crate::context::load_library;
use crate::domain::profiles::Row;
use crate::error::AppError;
use crate::games;
use crate::profiles::actions::save_profile;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

/// Mod Organizer 2 keeps a separator as a mod whose name ends with this.
const SEPARATOR_SUFFIX: &str = "_separator";

/// The longest name a separator may have in Gobbo.
const SEPARATOR_NAME_MAX: usize = 60;

/// One line of a `modlist.txt` that the import keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mo2Entry {
    /// A mod by the name of its folder in Mod Organizer's `mods` folder, and whether the profile has it on.
    Mod { name: String, enabled: bool },
    /// A separator, by the name in front of its suffix.
    Separator { name: String },
}

/// The entries of a `modlist.txt`, top line first. Comments, empty lines, the overwrite folder and the game's own files
/// (lines starting with `*`) are left out. A name listed again keeps its first place.
pub fn parse_modlist(text: &str) -> Vec<Mo2Entry> {
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('*') {
            continue;
        }
        // A line without a sign is a mod that is on.
        let enabled = !line.starts_with('-');
        let name = line.strip_prefix(['+', '-']).unwrap_or(line).trim();
        if name.is_empty()
            || name.eq_ignore_ascii_case("overwrite")
            || !seen.insert(name.to_lowercase())
        {
            continue;
        }
        if let Some(title) = name.strip_suffix(SEPARATOR_SUFFIX) {
            let title: String = title.trim().chars().take(SEPARATOR_NAME_MAX).collect();
            if !title.is_empty() {
                entries.push(Mo2Entry::Separator { name: title });
            }
        } else {
            entries.push(Mo2Entry::Mod {
                name: name.to_string(),
                enabled,
            });
        }
    }
    entries
}

/// What an import did to the active profile's list.
#[derive(Debug, Serialize, specta::Type)]
pub struct Mo2Import {
    /// Mods now in the list.
    pub mods: u32,
    /// Separators now in the list.
    pub separators: u32,
    /// Names from the file that the library does not have. They are left out of the list.
    pub missing: Vec<String>,
}

/// Makes the active profile's list the one of a Mod Organizer 2 `modlist.txt`: its mods, on or off, in its order, and its
/// separators. A name matches a library folder when the letters and digits agree. The change goes into the undo history
/// like any other list edit.
pub fn import_mo2_list(root: &Data, game_id: &str, text: &str) -> Result<Mo2Import, AppError> {
    let game = games::load(game_id)?;
    let library = load_library(&root.game(game_id), &game)?;
    // Normalize the library folders once, not once per line of the list. The first folder wins, as it did when the
    // lookup walked `library.versions` for every line.
    let mut by_key: HashMap<String, (&String, &crate::library::ModVersion)> =
        HashMap::with_capacity(library.versions.len());
    for (folder, record) in &library.versions {
        by_key
            .entry(crate::domain::naming::match_key(folder))
            .or_insert((folder, record));
    }
    let mut rows = Vec::new();
    let mut families = BTreeSet::new();
    let mut report = Mo2Import {
        mods: 0,
        separators: 0,
        missing: Vec::new(),
    };
    for entry in parse_modlist(text) {
        match entry {
            Mo2Entry::Separator { name } => {
                rows.push(Row {
                    folder: String::new(),
                    enabled: true,
                    separator: Some(name),
                });
                report.separators += 1;
            }
            Mo2Entry::Mod { name, enabled } => {
                let wanted = crate::domain::naming::match_key(&name);
                match by_key.get(&wanted) {
                    None => report.missing.push(name),
                    // Another version of a mod already in the list keeps the place of the first one.
                    Some(&(folder, record)) => {
                        if families.insert(record.family.clone()) {
                            rows.push(Row {
                                folder: folder.clone(),
                                enabled,
                                separator: None,
                            });
                            report.mods += 1;
                        }
                    }
                }
            }
        }
    }
    save_profile(root, game_id, &rows)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_keeps_its_order_and_states_and_leaves_out_what_gobbo_does_not_list() {
        let text = "# This file was automatically generated by Mod Organizer.\r\n+ModA\r\n-ModB\r\n*Example.esm\r\n\r\n+overwrite\r\n+Cool Stuff_separator\r\n+ModC\r\n";
        assert_eq!(
            parse_modlist(text),
            vec![
                Mo2Entry::Mod {
                    name: "ModA".into(),
                    enabled: true
                },
                Mo2Entry::Mod {
                    name: "ModB".into(),
                    enabled: false
                },
                Mo2Entry::Separator {
                    name: "Cool Stuff".into()
                },
                Mo2Entry::Mod {
                    name: "ModC".into(),
                    enabled: true
                },
            ]
        );
    }

    #[test]
    fn a_name_listed_twice_keeps_its_first_place_and_a_nameless_separator_is_skipped() {
        assert_eq!(
            parse_modlist("+A\n-A\n+_separator\nB\n"),
            vec![
                Mo2Entry::Mod {
                    name: "A".into(),
                    enabled: true
                },
                Mo2Entry::Mod {
                    name: "B".into(),
                    enabled: true
                },
            ]
        );
    }

    #[test]
    fn a_separator_name_is_cut_to_what_gobbo_allows() {
        let text = format!("+{}_separator\n", "x".repeat(80));
        assert!(matches!(
            parse_modlist(&text).as_slice(),
            [Mo2Entry::Separator { name }] if name.chars().count() == SEPARATOR_NAME_MAX
        ));
    }
}
