//! Indexes over the profiles of one game: which profiles list a family, and which select a version. The status and
//! the mod views ask these questions once per mod; a linear scan of every profile and every list for each question was
//! quadratic in the size of a large list.

use rustc_hash::{FxHashMap, FxHashSet};

use super::{Profile, Profiles};

/// A borrowed index over a `Profiles` value. Most games have one profile: then a family or version can only be in that
/// one list, so the sets replace thousands of one-element `Vec`s.
pub struct ProfilesView<'a> {
    by_family: FxHashMap<&'a str, Vec<&'a Profile>>,
    by_version: FxHashMap<&'a str, Vec<&'a Profile>>,
    single: Option<SingleProfile<'a>>,
    /// The collections of the active profile each family belongs to, by name.
    collections: FxHashMap<&'a str, Vec<&'a str>>,
}

struct SingleProfile<'a> {
    profile: &'a Profile,
    families: FxHashSet<&'a str>,
    versions: FxHashSet<&'a str>,
}

/// The one profile when the game has only one and it contains `key`, otherwise the indexed list.
fn one_or_many<'a, 'b>(
    single: Option<&'b SingleProfile<'a>>,
    list: &'b FxHashMap<&'a str, Vec<&'a Profile>>,
    key: &str,
    contains: impl Fn(&SingleProfile<'a>) -> bool,
) -> &'b [&'a Profile] {
    if let Some(single) = single {
        return if contains(single) {
            std::slice::from_ref(&single.profile)
        } else {
            &[]
        };
    }
    list.get(key).map_or(&[], Vec::as_slice)
}

impl<'a> ProfilesView<'a> {
    /// Builds the indexes in one pass over all profiles and their lists. A profile appears once for a family even when
    /// its list contains the family twice, matching the old `with_mod` lookup.
    pub fn new(profiles: &'a Profiles) -> Self {
        let mut collections: FxHashMap<&'a str, Vec<&'a str>> = FxHashMap::default();
        if let Some(active) = profiles
            .profiles
            .iter()
            .find(|profile| profile.id == profiles.active)
        {
            for origin in &active.collections {
                for family in &origin.families {
                    collections
                        .entry(family.as_str())
                        .or_default()
                        .push(origin.name.as_str());
                }
            }
        }
        if let [profile] = profiles.profiles.as_slice() {
            let mut families = FxHashSet::default();
            let mut versions = FxHashSet::default();
            families.reserve(profile.mods.len());
            versions.reserve(profile.mods.len());
            for entry in &profile.mods {
                families.insert(entry.id.as_str());
                versions.insert(entry.folder.as_str());
            }
            return Self {
                by_family: FxHashMap::default(),
                by_version: FxHashMap::default(),
                single: Some(SingleProfile {
                    profile,
                    families,
                    versions,
                }),
                collections,
            };
        }
        let mut by_family: FxHashMap<&'a str, Vec<&'a Profile>> = FxHashMap::default();
        let mut by_version: FxHashMap<&'a str, Vec<&'a Profile>> = FxHashMap::default();
        for profile in &profiles.profiles {
            let mut seen_families = FxHashSet::default();
            let mut seen_versions = FxHashSet::default();
            for entry in &profile.mods {
                if seen_families.insert(entry.id.as_str()) {
                    by_family
                        .entry(entry.id.as_str())
                        .or_default()
                        .push(profile);
                }
                if seen_versions.insert(entry.folder.as_str()) {
                    by_version
                        .entry(entry.folder.as_str())
                        .or_default()
                        .push(profile);
                }
            }
        }
        Self {
            by_family,
            by_version,
            single: None,
            collections,
        }
    }

    /// The profiles whose list has the family, in profile order.
    pub fn with_mod(&self, family: &str) -> &[&'a Profile] {
        one_or_many(self.single.as_ref(), &self.by_family, family, |single| {
            single.families.contains(family)
        })
    }

    /// The names of the active profile's collections that list the family, in the order they were installed.
    pub fn collections_of(&self, family: &str) -> &[&'a str] {
        self.collections.get(family).map_or(&[], Vec::as_slice)
    }

    /// The profiles that select the version folder, in profile order.
    pub fn using_version(&self, folder: &str) -> &[&'a Profile] {
        one_or_many(self.single.as_ref(), &self.by_version, folder, |single| {
            single.versions.contains(folder)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{profile, profiles};

    #[test]
    fn families_and_versions_are_indexed_once() {
        let profiles = profiles(
            "p1",
            vec![
                profile("p1", &[("a", "a1"), ("b", "b1"), ("a", "a2")]),
                profile("p2", &[("a", "a2")]),
            ],
        );
        let view = ProfilesView::new(&profiles);
        assert_eq!(
            view.with_mod("a")
                .iter()
                .map(|profile| profile.id.as_str())
                .collect::<Vec<_>>(),
            ["p1", "p2"]
        );
        assert_eq!(
            view.using_version("a2")
                .iter()
                .map(|profile| profile.id.as_str())
                .collect::<Vec<_>>(),
            ["p1", "p2"]
        );
        assert!(view.with_mod("missing").is_empty());
    }

    #[test]
    fn one_profile_answers_from_its_own_sets() {
        let profiles = profiles(
            "p1",
            vec![profile("p1", &[("a", "a1"), ("b", "b1"), ("a", "a2")])],
        );
        let view = ProfilesView::new(&profiles);
        assert_eq!(view.with_mod("a").len(), 1);
        assert_eq!(view.with_mod("a")[0].id, "p1");
        assert!(view.with_mod("missing").is_empty());
        assert_eq!(view.using_version("a2").len(), 1);
        assert!(view.using_version("missing").is_empty());
    }
}
