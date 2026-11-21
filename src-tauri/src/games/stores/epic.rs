//! The Epic Games launcher keeps one `.item` manifest per install in `<data>/Manifests`, where `<data>` is the folder
//! the launcher writes to the registry (`AppDataPath`), and an older list in `UnrealEngineLauncher/LauncherInstalled.dat`.
//! Both are read, as MO2 and others do; a game named by both is one install, which the scan keeps once.

use std::fs;

use serde_json::Value;

use super::{Candidate, IdKind, Store};
use crate::platform::StoreFolders;

/// Reads the manifests and the launcher list the folders name. A missing or broken file adds nothing.
pub fn read(out: &mut Vec<Candidate>, folders: &StoreFolders) {
    if let Some(data) = &folders.epic_data
        && let Ok(entries) = fs::read_dir(data.join("Manifests"))
    {
        let mut manifests: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("item"))
            })
            .collect();
        // The folder order is the file system's; sorting keeps the scan's answer the same on every run.
        manifests.sort();

        for path in manifests {
            if let Ok(text) = fs::read_to_string(&path)
                && let Some(candidate) = item_manifest(&text)
            {
                out.push(candidate);
            }
        }
    }

    if let Some(list) = &folders.epic_list
        && let Ok(text) = fs::read_to_string(list.join("LauncherInstalled.dat"))
    {
        out.extend(installed_list(&text));
    }
}

/// The install a `.item` manifest names. An unfinished download (`bIsIncompleteInstall`) and a DLC (its
/// `MainGameAppName` is another app) are not candidates: a DLC lives in its game's folder and has no install of its own.
pub fn item_manifest(text: &str) -> Option<Candidate> {
    let value = serde_json::from_str::<Value>(text).ok()?;

    if value.get("bIsIncompleteInstall").and_then(Value::as_bool) == Some(true) {
        return None;
    }

    let app = value.get("AppName")?.as_str()?;

    if let Some(main) = value.get("MainGameAppName").and_then(Value::as_str)
        && !main.is_empty()
        && main != app
    {
        return None;
    }

    candidate(app, value.get("InstallLocation")?.as_str()?)
}

/// The installs named by a launcher list: `{ "InstallationList": [{ "AppName", "InstallLocation" }] }`.
pub fn installed_list(text: &str) -> Vec<Candidate> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };

    value
        .get("InstallationList")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            candidate(
                item.get("AppName")?.as_str()?,
                item.get("InstallLocation")?.as_str()?,
            )
        })
        .collect()
}

/// An Epic candidate. An empty app name or folder is no install.
fn candidate(app: &str, root: &str) -> Option<Candidate> {
    let (app, root) = (app.trim(), root.trim());

    if app.is_empty() || root.is_empty() {
        return None;
    }

    Some(Candidate {
        store: Store::Epic,
        root: root.into(),
        id: app.to_string(),
        kind: IdKind::Epic,
        prefix: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{scratch, write};
    use std::path::Path;

    /// A small, realistic Epic manifest: most of its fields are irrelevant to detection.
    const ITEM: &str = r#"{
        "FormatVersion": 0,
        "bIsIncompleteInstall": false,
        "AppName": "SlotApp",
        "MainGameAppName": "SlotApp",
        "CatalogNamespace": "slotnamespace",
        "AppVersion": "1.2.3",
        "InstallLocation": "/games/Epic/Slot",
        "DisplayName": "Slot Game",
        "LaunchExecutable": "Binaries/Game.exe"
    }"#;

    #[test]
    fn an_item_manifest_names_the_app_and_the_install_folder() {
        let candidate = item_manifest(ITEM).expect("the manifest has both fields");
        assert_eq!(candidate.store, Store::Epic);
        assert_eq!(candidate.kind, IdKind::Epic);
        assert_eq!(candidate.id, "SlotApp");
        assert_eq!(candidate.root, Path::new("/games/Epic/Slot"));
    }

    #[test]
    fn an_unfinished_download_and_a_dlc_are_not_installs() {
        let incomplete = ITEM.replace(
            r#""bIsIncompleteInstall": false"#,
            r#""bIsIncompleteInstall": true"#,
        );
        assert!(item_manifest(&incomplete).is_none());

        let dlc = ITEM.replace(r#""AppName": "SlotApp""#, r#""AppName": "SlotDlc""#);
        assert!(item_manifest(&dlc).is_none());

        // An older manifest without `MainGameAppName` is still a game.
        let old = ITEM.replace(r#""MainGameAppName": "SlotApp","#, "");
        assert_eq!(
            item_manifest(&old).map(|found| found.id),
            Some("SlotApp".into())
        );
    }

    #[test]
    fn a_manifest_missing_a_field_or_without_json_is_skipped() {
        assert!(item_manifest(r#"{"AppName":"SlotApp"}"#).is_none());
        assert!(item_manifest(r#"{"InstallLocation":"/games/Epic/Slot"}"#).is_none());
        assert!(item_manifest(r#"{"AppName":"","InstallLocation":"/games/Epic/Slot"}"#).is_none());
        assert!(item_manifest("not json").is_none());
    }

    #[test]
    fn the_manifests_and_the_launcher_list_are_both_read() {
        let base = scratch!("epic-lists");
        let folders = StoreFolders {
            epic_data: Some(base.join("EpicGamesLauncher/Data")),
            epic_list: Some(base.join("UnrealEngineLauncher")),
            ..StoreFolders::default()
        };
        write(
            &base.join("UnrealEngineLauncher/LauncherInstalled.dat"),
            &serde_json::json!({
                "InstallationList": [{
                    "AppName": "ListApp",
                    "InstallLocation": "/games/Epic/List",
                }],
            })
            .to_string(),
        );
        write(
            &base.join("EpicGamesLauncher/Data/Manifests/slot.item"),
            ITEM,
        );
        write(
            &base.join("EpicGamesLauncher/Data/Manifests/other.txt"),
            "not a manifest",
        );

        let mut out = Vec::new();
        read(&mut out, &folders);
        let ids: Vec<_> = out.iter().map(|candidate| candidate.id.as_str()).collect();
        assert_eq!(ids, ["SlotApp", "ListApp"]);
    }

    #[test]
    fn without_the_launcher_folders_epic_names_no_candidate() {
        let mut out = Vec::new();
        read(&mut out, &StoreFolders::default());
        assert!(out.is_empty());
    }
}
