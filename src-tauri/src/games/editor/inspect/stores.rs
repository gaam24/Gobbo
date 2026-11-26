//! The store ids a game folder carries: the Steam manifest in its library and the ids other stores recorded.

use std::fs;
use std::path::Path;

use crate::domain::pathkey::same_path;
use crate::games::paths::home;
use crate::games::stores::steam::vdf_values;
use crate::games::stores::{IdKind, candidates};

use super::super::{SteamSuggestion, StoreIds};
use super::folder::sorted_entries;

pub(super) fn steam_manifest(folder: &Path) -> Option<SteamSuggestion> {
    let common = folder.parent()?;

    if common.file_name()? != "common" {
        return None;
    }

    let steamapps = common.parent()?;

    if steamapps.file_name()? != "steamapps" {
        return None;
    }

    let folder_name = folder.file_name()?.to_string_lossy();

    for entry in sorted_entries(steamapps) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_manifest = name.starts_with("appmanifest_")
            && Path::new(&name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("acf"));
        if !is_manifest {
            continue;
        }

        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let install_dir = vdf_values(&text, "installdir").into_iter().next();
        if install_dir
            .as_deref()
            .is_some_and(|dir| dir.eq_ignore_ascii_case(&folder_name))
        {
            return Some(SteamSuggestion {
                app_id: vdf_values(&text, "appid")
                    .first()
                    .and_then(|value| value.parse().ok())?,
                name: vdf_values(&text, "name").into_iter().next(),
                install_dir,
            });
        }
    }

    None
}

pub(super) fn store_ids(folder: &Path) -> StoreIds {
    let mut ids = StoreIds::default();
    let home = home();

    for candidate in candidates(&home) {
        if !same_path(&candidate.root, folder) {
            continue;
        }

        // The id goes to the list it belongs to: a GOG game installed through Heroic or Lutris is a GOG id.
        let list = match candidate.kind {
            IdKind::Gog => &mut ids.gog,
            IdKind::Epic => &mut ids.epic,
            #[cfg(not(windows))]
            IdKind::Lutris => &mut ids.lutris,
        };

        if !list.contains(&candidate.id) {
            list.push(candidate.id);
        }
    }

    ids
}
