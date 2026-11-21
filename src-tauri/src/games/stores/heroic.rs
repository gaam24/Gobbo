//! Heroic keeps its installs in two lists: Legendary's map for Epic games (`legendaryConfig/legendary/installed.json`)
//! and its own GOG list (`gog_store/installed.json`, `{ "installed": [...] }`). Standalone Legendary keeps the same
//! map in its own config folder. Heroic also keeps one settings file per game in `GamesConfig`, which names the Wine
//! prefix it runs the game in. DLC entries share their game's folder and are skipped.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Candidate, IdKind, Store};

/// Reads Heroic's Epic and GOG install lists from one of its config folders, each with the prefix Heroic runs it in.
pub fn read(out: &mut Vec<Candidate>, heroic_dir: &Path) {
    let mut found = Vec::new();

    if let Ok(text) =
        fs::read_to_string(heroic_dir.join("legendaryConfig/legendary/installed.json"))
    {
        found.extend(legendary(&text, Store::Heroic));
    }

    if let Ok(text) = fs::read_to_string(heroic_dir.join("gog_store/installed.json")) {
        found.extend(gog(&text));
    }

    for candidate in &mut found {
        let config = heroic_dir.join(format!("GamesConfig/{}.json", candidate.id));
        candidate.prefix = fs::read_to_string(config)
            .ok()
            .and_then(|text| game_prefix(&text, &candidate.id));
    }

    out.extend(found);
}

/// The Wine prefix of one game's `GamesConfig` file: `{ "<app>": { "winePrefix", "wineVersion": { "type" } } }`. Under
/// Proton the folder Heroic names is Proton's data folder, and the prefix itself is its `pfx`; under Wine it is the
/// prefix. A file without a prefix names none, and the user can pick the prefix in the game's settings.
pub fn game_prefix(text: &str, app: &str) -> Option<PathBuf> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let game = value.get(app)?;
    let folder = game.get("winePrefix")?.as_str()?.trim();

    if folder.is_empty() {
        return None;
    }

    let proton = game
        .get("wineVersion")
        .and_then(|version| version.get("type"))
        .and_then(Value::as_str)
        == Some("proton");
    let folder = PathBuf::from(folder);
    Some(if proton { folder.join("pfx") } else { folder })
}

/// Reads a standalone Legendary install list from its own config folder.
pub fn read_legendary(out: &mut Vec<Candidate>, legendary_dir: &Path) {
    if let Ok(text) = fs::read_to_string(legendary_dir.join("installed.json")) {
        out.extend(legendary(&text, Store::Legendary));
    }
}

/// The Epic installs of a Legendary `installed.json`: a map of `AppName` to `{ install_path, is_dlc }`, managed by
/// `store`.
pub fn legendary(text: &str, store: Store) -> Vec<Candidate> {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };

    map.into_iter()
        .filter_map(|(id, value)| install(store, IdKind::Epic, id, &value))
        .collect()
}

/// The GOG installs of Heroic's GOG list: `{ "installed": [{ appName, install_path, is_dlc }] }`.
pub fn gog(text: &str) -> Vec<Candidate> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };

    value
        .get("installed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = item.get("appName")?.as_str()?.to_string();
            install(Store::Heroic, IdKind::Gog, id, item)
        })
        .collect()
}

/// One entry of an install list: its folder from `install_path`. A DLC or an entry without a folder is no install.
fn install(store: Store, kind: IdKind, id: String, entry: &Value) -> Option<Candidate> {
    if entry.get("is_dlc").and_then(Value::as_bool) == Some(true) || id.trim().is_empty() {
        return None;
    }

    let root = entry.get("install_path")?.as_str()?.trim();
    (!root.is_empty()).then(|| Candidate {
        store,
        root: PathBuf::from(root),
        id,
        kind,
        prefix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{scratch, write};

    #[test]
    fn a_legendary_map_names_each_app_and_its_install_folder() {
        let candidates = legendary(
            &serde_json::json!({
                "SlotApp": { "install_path": "/games/Heroic/Slot", "platform": "Windows", "is_dlc": false },
                "SlotDlc": { "install_path": "/games/Heroic/Slot", "is_dlc": true },
                "NoPath": { "platform": "Windows" },
            })
            .to_string(),
            Store::Heroic,
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].store, Store::Heroic);
        assert_eq!(candidates[0].kind, IdKind::Epic);
        assert_eq!(candidates[0].id, "SlotApp");
        assert_eq!(candidates[0].root, Path::new("/games/Heroic/Slot"));
    }

    #[test]
    fn the_gog_list_names_each_game_and_its_install_folder() {
        // The shape Heroic writes: the list sits under `installed`.
        let candidates = gog(&serde_json::json!({
            "installed": [
                { "appName": "GogApp", "install_path": "/games/Heroic/Gog", "platform": "windows", "is_dlc": false },
                { "appName": "GogDlc", "install_path": "/games/Heroic/Gog", "is_dlc": true },
                { "appName": "MissingPath" },
            ],
        })
        .to_string());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].store, Store::Heroic);
        assert_eq!(candidates[0].kind, IdKind::Gog);
        assert_eq!(candidates[0].id, "GogApp");
        assert_eq!(candidates[0].root, Path::new("/games/Heroic/Gog"));
    }

    #[test]
    fn a_broken_or_wrongly_shaped_file_names_nothing() {
        assert!(legendary("not json", Store::Heroic).is_empty());
        assert!(legendary("[]", Store::Heroic).is_empty());
        assert!(gog("{}").is_empty());
        assert!(gog(r#"{"installed": {}}"#).is_empty());
    }

    #[test]
    fn a_games_config_names_the_wine_prefix_or_the_pfx_of_a_proton_folder() {
        let wine = serde_json::json!({
            "SlotApp": { "winePrefix": "/prefixes/Slot", "wineVersion": { "type": "wine" } },
        })
        .to_string();
        assert_eq!(
            game_prefix(&wine, "SlotApp"),
            Some(PathBuf::from("/prefixes/Slot"))
        );

        let proton = serde_json::json!({
            "SlotApp": { "winePrefix": "/prefixes/Slot", "wineVersion": { "type": "proton" } },
        })
        .to_string();
        assert_eq!(
            game_prefix(&proton, "SlotApp"),
            Some(PathBuf::from("/prefixes/Slot/pfx"))
        );

        assert_eq!(game_prefix(&proton, "OtherApp"), None);
        assert_eq!(
            game_prefix(r#"{"SlotApp":{"winePrefix":""}}"#, "SlotApp"),
            None
        );
        assert_eq!(game_prefix("not json", "SlotApp"), None);
    }

    #[test]
    fn a_heroic_install_carries_the_prefix_of_its_games_config() {
        let base = scratch!("heroic-prefix");
        let heroic = base.join("config/heroic");
        write(
            &heroic.join("gog_store/installed.json"),
            &serde_json::json!({ "installed": [
                { "appName": "123", "install_path": "/games/Heroic/Slot" },
                { "appName": "456", "install_path": "/games/Heroic/Other" },
            ] })
            .to_string(),
        );
        write(
            &heroic.join("GamesConfig/123.json"),
            &serde_json::json!({
                "123": { "winePrefix": "/prefixes/Slot", "wineVersion": { "type": "proton" } },
            })
            .to_string(),
        );

        let mut out = Vec::new();
        read(&mut out, &heroic);
        let prefix_of = |id: &str| {
            out.iter()
                .find(|candidate| candidate.id == id)
                .and_then(|candidate| candidate.prefix.clone())
        };
        assert_eq!(prefix_of("123"), Some(PathBuf::from("/prefixes/Slot/pfx")));
        assert_eq!(prefix_of("456"), None);
    }

    #[test]
    fn heroic_and_standalone_legendary_are_read_from_their_config_folders() {
        let base = scratch!("heroic-config");
        let heroic = base.join("config/heroic");
        let standalone = base.join("config/legendary");
        write(
            &heroic.join("legendaryConfig/legendary/installed.json"),
            &serde_json::json!({
                "HeroicApp": { "install_path": "/games/Heroic/Slot" },
            })
            .to_string(),
        );
        write(
            &heroic.join("gog_store/installed.json"),
            &serde_json::json!({ "installed": [
                { "appName": "HeroicGog", "install_path": "/games/Heroic/Gog" },
            ] })
            .to_string(),
        );
        write(
            &standalone.join("installed.json"),
            &serde_json::json!({
                "LegendaryApp": { "install_path": "/games/Legendary/Slot" },
            })
            .to_string(),
        );

        let mut out = Vec::new();
        read(&mut out, &heroic);
        read_legendary(&mut out, &standalone);
        let found: Vec<_> = out
            .iter()
            .map(|candidate| (candidate.id.as_str(), candidate.store, candidate.kind))
            .collect();
        assert_eq!(
            found,
            [
                ("HeroicApp", Store::Heroic, IdKind::Epic),
                ("HeroicGog", Store::Heroic, IdKind::Gog),
                ("LegendaryApp", Store::Legendary, IdKind::Epic),
            ]
        );
    }
}
