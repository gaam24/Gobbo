//! The mods of a pack that wait for their archive. `sync_profiles` needs it on every status read, and the packs module
//! uses it while importing, so it lives with the profile list rather than in the pack reader.

use crate::domain::library::LibraryView;
use crate::domain::pack::PackMod;
use crate::domain::profiles::Entry as ProfileEntry;
use crate::domain::profiles::Profile;

/// The library family and folder a pack mod stands for, by the shared rule in `LibraryView::find`. A mod with no
/// version in the pack takes the newest one; a mod whose version the library lacks is not found.
pub fn find_version(view: &LibraryView, entry: &PackMod) -> Option<(String, String)> {
    view.find(entry, None)?.entry()
}

/// Puts the waiting mods of a profile in its list as soon as the library has them. Each one goes right above the mod it
/// sat above in the pack, and every name of it in the list (separators, file winners, groups, the anchors of the other
/// waiting mods) moves to the family it joins as. A mod the list has already is only dropped from the waiting ones.
/// Returns the families that left the waiting list; empty when nothing changed.
pub fn settle_waiting(profile: &mut Profile, view: &LibraryView) -> Vec<String> {
    let mut settled = Vec::new();
    // The waiting mods are settled one at a time, so a mod that joins first is already in the list when a later one
    // sits above it, and the names of the rest follow it.
    let mut at = 0;

    while at < profile.waiting.len() {
        let Some((family, folder)) = find_version(view, &profile.waiting[at].entry) else {
            at += 1;
            continue;
        };
        let waiting = profile.waiting.remove(at);
        settled.push(family.clone());
        profile.retarget_family(waiting.key(), &family);

        if profile.mods.iter().any(|entry| entry.id == family) {
            continue;
        }

        let position = waiting
            .before
            .as_ref()
            .and_then(|id| profile.mods.iter().position(|entry| entry.id == *id))
            .unwrap_or(profile.mods.len());
        profile.mods.insert(
            position,
            ProfileEntry {
                id: family,
                folder,
                enabled: waiting.entry.enabled,
                priority: waiting.entry.priority,
            },
        );
    }

    settled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::library::{Library, ModInfo, ModInfoEdit, ModVersion};
    use std::collections::BTreeMap;

    fn archived(folder: &str, sha: &str) -> ModVersion {
        ModVersion {
            folder: folder.into(),
            family: format!("family-{folder}"),
            slot: String::new(),
            files: Vec::new().into(),
            size_bytes: 0,
            info: ModInfo {
                name: "Generic".into(),
                version: None,
                author: None,
                description: None,
            },
            edit: ModInfoEdit::default(),
            archive: Some(sha.into()),
            parsed_sources: BTreeMap::new(),
            parsed_parts: BTreeMap::new(),
            installed_at: 0,
            fomod: Vec::new(),
            fomod_installer: false,
            integrity: None,
        }
    }

    #[test]
    fn a_pack_version_label_that_differs_from_the_archive_still_finds_its_file() {
        let mut library = Library::default();
        let mut version = archived("Example Mod", "sha");
        version.family = "nexus:12345".into();
        version.info.name = "Example Mod".into();
        version.info.version = Some("2.0".into());
        version.parsed_sources = BTreeMap::from([("nexus".into(), "12345".into())]);
        library.versions.insert("Example Mod".into(), version);
        library.archives.insert(
            "sha".into(),
            crate::domain::library::Archive {
                original_name: "Example Mod 12345 2.0 2026-01-01T00-00Z XyZzyxwv.zip".into(),
                file: "Example Mod 12345 2.0 2026-01-01T00-00Z XyZzyxwv.zip".into(),
                named: None,
                source_files: BTreeMap::from([("nexus".into(), "99999".into())]),
                source_parts: BTreeMap::new(),
                skipped: Vec::new(),
                skipped_total: 0,
            },
        );
        let view = LibraryView::new(&library);
        // The collection names 2.0.1 while the archive is named 2.0. The shared file id still resolves it.
        let entry: PackMod = serde_json::from_str(
            r#"{"name":"Example Mod","version":"2.0.1","family":"nexus:12345","enabled":true,"sources":{"nexus":{"mod_id":"12345"}},"source_files":{"nexus":"99999"}}"#,
        )
        .unwrap();

        assert_eq!(
            find_version(&view, &entry),
            Some(("nexus:12345".to_string(), "Example Mod".to_string()))
        );
    }

    #[test]
    fn a_fresh_import_wins_over_an_older_version_of_the_same_archive() {
        let mut library = Library::default();
        library
            .versions
            .insert("a-old".into(), archived("a-old", "sha"));
        library
            .versions
            .insert("z-pack".into(), archived("z-pack", "sha"));
        let view = LibraryView::new(&library);
        // The pack names the mod; both installed folders carry a generic name from `info.json`.
        let entry: PackMod = serde_json::from_str(
            r#"{"name":"The Pack Mod","version":null,"family":"family-z-pack","enabled":true,"archive":{"name":"pack.zip","sha256":"sha","size":0}}"#,
        )
        .unwrap();

        assert_eq!(
            find_version(&view, &entry).map(|(_, folder)| folder),
            Some("a-old".into()),
            "without a hint the first folder in the library wins"
        );
        let fresh = BTreeMap::from([("sha".to_string(), vec!["z-pack".to_string()])]);
        assert_eq!(
            view.find(&entry, Some(&fresh))
                .and_then(|found| found.entry())
                .map(|(_, folder)| folder),
            Some("z-pack".into())
        );
    }
}
