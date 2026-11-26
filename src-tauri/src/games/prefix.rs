//! The store an install came from, and the Proton prefix its paths resolve with. A Steam install uses the prefix Steam
//! keeps in its library; every other Windows install on Linux uses the prefix the user chose, or one Gobbo owns in the
//! game's data folder.

use std::path::{Path, PathBuf};

use super::GameDef;
use super::runtime::Runtime;
use super::stores::Store;
use super::stores::steam;
use crate::context::Data;

/// The effective store of an install: the saved source, or a fallback for installs saved before the source existed.
/// An old install with a Steam app id is Steam; one without is a manual folder. A saved name that names no store is
/// treated as a manual folder, so an unknown source never turns into a Steam install.
pub fn effective_source(source: &str, steam_app_ids: &[u32]) -> Store {
    if !source.is_empty() {
        return Store::from_name(source).unwrap_or_else(|| {
            crate::infra::log::warn(format!(
                "unknown saved install source {source:?}; using manual"
            ));
            Store::Manual
        });
    }

    if steam_app_ids.is_empty() {
        Store::Manual
    } else {
        Store::Steam
    }
}

/// The folder Gobbo owns for a game's Windows data: `<data>/games/<game-id>/compatdata`. It travels with the game's data
/// when a game is removed or restored, and snapshots capture profile sets only, so a multi-GB prefix is never copied.
pub fn owned_compatdata(data: &Data, game_id: &str) -> PathBuf {
    data.root()
        .join(crate::migrations::GAMES_DIR)
        .join(game_id)
        .join("compatdata")
}

/// The Wine prefix of an install that is not from Steam: the folder the user chose (Gobbo fills it in from Heroic or
/// Lutris when the store names one), otherwise the one Gobbo owns. It may not exist yet.
pub fn other_prefix(data: &Data, game_id: &str, prefix_dir: &str) -> PathBuf {
    let chosen = prefix_dir.trim();

    if chosen.is_empty() {
        owned_compatdata(data, game_id).join("pfx")
    } else {
        PathBuf::from(chosen)
    }
}

/// The prefix a path template of this install resolves with. A Steam install under Proton uses the prefix in its Steam
/// library; another Windows install uses [`other_prefix`] once it exists. A native install has no prefix.
pub fn prefix_of(
    game: &GameDef,
    runtime: Runtime,
    game_root: &Path,
    prefix_dir: &str,
    data: &Data,
) -> Option<PathBuf> {
    match runtime {
        Runtime::SteamProton => steam::proton_prefix(game, game_root),
        Runtime::OwnedProton => {
            let prefix = other_prefix(data, &game.id, prefix_dir);
            prefix.is_dir().then_some(prefix)
        }
        Runtime::Native => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games as test_games;
    use crate::test_support::scratch;
    use std::fs;

    #[test]
    fn an_empty_source_falls_back_to_steam_only_with_an_app_id() {
        let ids = [1001];
        assert_eq!(effective_source("gog", &ids), Store::Gog);
        assert_eq!(effective_source("steam", &[]), Store::Steam);
        assert_eq!(effective_source("", &ids), Store::Steam);
        assert_eq!(effective_source("", &[]), Store::Manual);
        // An unknown saved name stays a non-Steam install rather than inheriting the app id's store.
        assert_eq!(effective_source("unknown", &ids), Store::Manual);
    }

    #[test]
    fn an_unknown_saved_source_stays_manual() {
        let ids = [1001];
        assert_eq!(effective_source("unknown", &ids), Store::Manual);
        assert_eq!(effective_source("not-a-store", &[]), Store::Manual);
    }

    #[test]
    fn an_owned_prefix_exists_only_once_the_game_has_run() {
        let base = scratch!("owned-prefix");
        let data = Data::new(base.join("data"));
        let game = test_games::mirror();
        let game_root = base.join("game");
        let compatdata = owned_compatdata(&data, &game.id);
        assert_eq!(compatdata, data.game(&game.id).dir.join("compatdata"));
        let owned = Runtime::OwnedProton;
        assert_eq!(prefix_of(&game, owned, &game_root, "", &data), None);

        let prefix = compatdata.join("pfx");
        fs::create_dir_all(&prefix).unwrap();
        assert_eq!(prefix_of(&game, owned, &game_root, "", &data), Some(prefix));
    }

    #[test]
    fn a_chosen_prefix_replaces_the_owned_one_and_a_native_install_has_none() {
        let base = scratch!("chosen-prefix");
        let data = Data::new(base.join("data"));
        let game = test_games::mirror();
        let game_root = base.join("game");
        let chosen = base.join("store/prefixes/Game/pfx");
        fs::create_dir_all(&chosen).unwrap();
        fs::create_dir_all(owned_compatdata(&data, &game.id).join("pfx")).unwrap();
        let text = format!("  {}  ", chosen.display());

        assert_eq!(other_prefix(&data, &game.id, &text), chosen);
        assert_eq!(
            prefix_of(&game, Runtime::OwnedProton, &game_root, &text, &data),
            Some(chosen)
        );
        assert_eq!(
            prefix_of(&game, Runtime::Native, &game_root, &text, &data),
            None
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn a_steam_install_still_resolves_its_prefix_from_the_library() {
        let base = scratch!("steam-prefix-source");
        let data = Data::new(base.join("data"));
        let game = test_games::slot();
        let library = base.join("library");
        let game_root = library.join("steamapps/common/Slot Game");
        let prefix = library.join("steamapps/compatdata/1001/pfx");
        fs::create_dir_all(&prefix).unwrap();
        assert_eq!(
            prefix_of(&game, Runtime::SteamProton, &game_root, "", &data),
            Some(prefix.clone())
        );
        // An owned prefix that happens to exist must not replace a Steam install's prefix.
        fs::create_dir_all(owned_compatdata(&data, &game.id).join("pfx")).unwrap();
        assert_eq!(
            prefix_of(&game, Runtime::SteamProton, &game_root, "", &data),
            Some(prefix)
        );
    }
}
