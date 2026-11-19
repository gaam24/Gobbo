//! Keeping every profile in step with the library: missing families, removed versions, separators and waiting mods.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::domain::profiles::Entry as ProfileEntry;
use crate::domain::profiles::Profiles;
use crate::library::{Library, LibraryView};

use super::waiting;

/// Every profile lists every mod family in the library. Families that a profile misses (a fresh import, a folder
/// adopted by a rescan, a newly created profile) are appended at the end, disabled, with the latest version.
/// A selection that points at a removed version falls back to the latest one. Returns whether anything changed.
pub fn sync_profiles(profiles: &mut Profiles, library: &Library, view: &LibraryView) -> bool {
    let mut changed = false;
    for profile in &mut profiles.profiles {
        // Removing entries and adjusting separators is only needed when a family disappeared. The common status
        // path then skips a full pass over the list.
        if profile
            .mods
            .iter()
            .any(|entry| !library.families.contains_key(&entry.id))
        {
            let before_count = profile.mods.len();
            profile.remove_mods(|entry| !library.families.contains_key(&entry.id));
            if profile.mods.len() != before_count {
                changed = true;
            }
        }
        if !waiting::settle_waiting(profile, view).is_empty() {
            changed = true;
        }
        for entry in &mut profile.mods {
            if !library.versions.contains_key(&entry.folder)
                && let Some(version) = view.default_version(&entry.id)
            {
                entry.folder = version.folder.clone();
                changed = true;
            }
        }
        // The present ids borrow the profile. Copying every id into a set was an allocation per mod on every
        // status; the set only answers membership questions here.
        let present: FxHashSet<&str> = profile.mods.iter().map(|entry| entry.id.as_str()).collect();
        for separator in &mut profile.separators {
            // A separator above a waiting mod names that mod until it joins the list.
            let anchored = separator.before.as_ref().is_none_or(|id| {
                present.contains(id.as_str())
                    || profile.waiting.iter().any(|waiting| waiting.key() == id)
            });
            if !anchored {
                separator.before = None;
                changed = true;
            }
        }
        let missing: Vec<&str> = library
            .families
            .keys()
            .filter(|id| !present.contains(id.as_str()))
            .map(String::as_str)
            .collect();
        drop(present);
        if missing.is_empty() {
            continue;
        }
        // Parts of one page stay together: a new part goes right after the last part of its page. The page map is built
        // once, so a fresh profile over a large library is linear instead of a scan of the growing list per mod.
        let mut page_last: FxHashMap<(&str, &str), usize> = FxHashMap::default();
        for (at, entry) in profile.mods.iter().enumerate() {
            if let Some(family) = library.families.get(&entry.id) {
                for (provider, source) in &family.sources {
                    page_last.insert((provider.as_str(), source.mod_id.as_str()), at);
                }
            }
        }
        let mut insertions: FxHashMap<usize, Vec<ProfileEntry>> = FxHashMap::default();
        for id in missing {
            let Some(version) = view.default_version(id) else {
                continue;
            };
            let Some(family) = library.families.get(id) else {
                continue;
            };
            let after = family
                .sources
                .iter()
                .filter_map(|(provider, source)| {
                    page_last
                        .get(&(provider.as_str(), source.mod_id.as_str()))
                        .map(|at| at + 1)
                })
                .max()
                .unwrap_or(profile.mods.len());
            insertions.entry(after).or_default().push(ProfileEntry {
                id: id.to_string(),
                folder: version.folder.clone(),
                enabled: false,
                priority: None,
            });
            changed = true;
        }
        if !insertions.is_empty() {
            let old = std::mem::take(&mut profile.mods);
            let end = old.len();
            let mut rebuilt =
                Vec::with_capacity(end + insertions.values().map(Vec::len).sum::<usize>());
            for (at, entry) in old.into_iter().enumerate() {
                rebuilt.push(entry);
                if let Some(entries) = insertions.remove(&(at + 1)) {
                    rebuilt.extend(entries);
                }
            }
            // A family without a page goes at the end.
            if let Some(entries) = insertions.remove(&end) {
                rebuilt.extend(entries);
            }
            profile.mods = rebuilt;
        }
    }
    changed
}
