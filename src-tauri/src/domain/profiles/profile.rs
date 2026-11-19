//! One profile: its list, the separators and the file winners, and the part a list edit changes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::collection::CollectionOrigin;
use crate::domain::pack::WaitingMod;
use crate::domain::user_data::Isolation;

use super::{diff, group::ModGroup, history};

/// A mod on a profile list. The order in the list decides which copy of a shared file wins.
/// `id` is the family, `folder` the version selected for this profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub folder: String,
    pub enabled: bool,
    /// The load priority the user set. It is written to the game's settings file only when set; `None` leaves the
    /// order to the game.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
}

/// A row of a profile list as the window sends it back: the selected version and whether it is enabled. The order is
/// the priority. A row with `separator` is a separator with that name, and its `folder` is the separator's id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Row {
    pub folder: String,
    pub enabled: bool,
    #[serde(default)]
    pub separator: Option<String>,
}

/// A named header in a profile's list that groups the mods below it, as in Mod Organizer 2. It sits right above the
/// mod `before` (a family id), or at the end of the list when `before` is `None`. Anchoring it to a mod keeps it in
/// place when mods are added at the end or removed elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Separator {
    pub id: String,
    pub name: String,
    pub before: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub mods: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub separators: Vec<Separator>,
    /// Groups of related families on the list. Each one holds at least two members.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<ModGroup>,
    /// The enabled entries as they were at the last deployment. Empty before the first one.
    #[serde(default)]
    pub deployed_mods: Vec<Entry>,
    /// Unix time in seconds of the last deployment of this profile.
    #[serde(default)]
    pub deployed_at: Option<u32>,
    /// The file winners the user chose: path key -> family id. Only `Mirror` uses them, see `resolver::plan_with`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub winners: BTreeMap<String, String>,
    /// The winners at the last deployment. A difference from `winners` is a change that waits for a deployment.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deployed_winners: BTreeMap<String, String>,
    /// The choice for each user-data set of the game: shared or isolated. A missing id means `shared`, so a profile that
    /// predates a set never starts isolating its data on its own. A new profile gets every set's default from
    /// `GameDef::user_data_defaults`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub user_data: BTreeMap<String, Isolation>,
    /// The pack this profile was made from, so a newer version of the pack updates it in place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackOrigin>,
    /// Nexus collections installed into this profile. A profile can hold several; a pack profile holds none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<CollectionOrigin>,
    /// Mods of the pack or of a collection that the library lacks. Each waits here with its place and state, and joins
    /// the list once the library has it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waiting: Vec<WaitingMod>,
}

/// The pack a profile was made from: its id and version, and the name of the profile in the pack (the profile here
/// may have another name, when that one was taken).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackOrigin {
    pub id: String,
    pub version: String,
    pub profile: String,
}

impl Profile {
    pub(crate) const fn new(id: String, name: String, mods: Vec<Entry>) -> Self {
        Self {
            id,
            name,
            mods,
            separators: Vec::new(),
            groups: Vec::new(),
            deployed_mods: Vec::new(),
            deployed_at: None,
            winners: BTreeMap::new(),
            deployed_winners: BTreeMap::new(),
            user_data: BTreeMap::new(),
            pack: None,
            collections: Vec::new(),
            waiting: Vec::new(),
        }
    }

    /// The isolation choice for a user-data set. A profile that predates the set shares it.
    pub fn user_data_isolation(&self, id: &str) -> Isolation {
        self.user_data.get(id).copied().unwrap_or_default()
    }

    /// Sets the isolation choice for a user-data set.
    pub fn set_user_data_isolation(&mut self, id: &str, isolation: Isolation) {
        self.user_data.insert(id.to_string(), isolation);
    }

    /// Adds the family at the end of the list, enabled, unless the list already has it. Returns whether it was added.
    pub fn add_mod(&mut self, family: &str, folder: &str) -> bool {
        if self.mods.iter().any(|entry| entry.id == family) {
            return false;
        }
        self.mods.push(Entry {
            id: family.to_string(),
            folder: folder.to_string(),
            enabled: true,
            priority: None,
        });
        true
    }

    /// The enabled entries, in list order.
    pub fn enabled_mods(&self) -> Vec<Entry> {
        self.mods
            .iter()
            .filter(|entry| entry.enabled)
            .cloned()
            .collect()
    }

    /// The library folders of the enabled entries, in list order.
    pub fn enabled_folders(&self) -> Vec<String> {
        self.mods
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.folder.clone())
            .collect()
    }

    /// How many entries of the list are enabled.
    pub fn enabled_count(&self) -> usize {
        self.mods.iter().filter(|entry| entry.enabled).count()
    }

    /// Renames the key `from` to `to` wherever the list names a mod by it. A waiting mod is named by its pack family
    /// until it joins the list; this moves every name to the family it joins as: the separators above it, the file
    /// winners that chose it, the groups it belongs to and the waiting mods that sat above it. A group that would name
    /// `to` twice keeps one member.
    pub fn retarget_family(&mut self, from: &str, to: &str) {
        for separator in &mut self.separators {
            if separator.before.as_deref() == Some(from) {
                separator.before = Some(to.to_string());
            }
        }
        for family in self.winners.values_mut() {
            if family == from {
                *family = to.to_string();
            }
        }
        for group in &mut self.groups {
            if !group.members.iter().any(|member| member == from) {
                continue;
            }
            let mut members: Vec<String> = Vec::with_capacity(group.members.len());
            for member in group.members.drain(..) {
                let member = if member == from {
                    to.to_string()
                } else {
                    member
                };
                if !members.contains(&member) {
                    members.push(member);
                }
            }
            group.members = members;
        }
        for waiting in &mut self.waiting {
            if waiting.before.as_deref() == Some(from) {
                waiting.before = Some(to.to_string());
            }
        }
    }

    /// Removes the entries `drop` picks. A separator above a removed mod moves to the next mod that stays, so a group
    /// keeps its place in the list. The deployment ledger and the deployed file winners forget the removed families
    /// too, so a later import of one does not inherit the old deployment.
    pub fn remove_mods(&mut self, mut drop: impl FnMut(&Entry) -> bool) {
        let dropped: Vec<bool> = self.mods.iter().map(&mut drop).collect();
        let removed: Vec<String> = self
            .mods
            .iter()
            .zip(&dropped)
            .filter(|(_, dropped)| **dropped)
            .map(|(entry, _)| entry.id.clone())
            .collect();
        for separator in &mut self.separators {
            let Some(at) = separator
                .before
                .as_ref()
                .and_then(|id| self.mods.iter().position(|entry| entry.id == *id))
            else {
                continue;
            };
            if dropped[at] {
                separator.before = (at..self.mods.len())
                    .find(|&index| !dropped[index])
                    .map(|index| self.mods[index].id.clone());
            }
        }
        let mut index = 0;
        self.mods.retain(|_| {
            index += 1;
            !dropped[index - 1]
        });
        self.deployed_mods
            .retain(|entry| !removed.contains(&entry.id));
        self.deployed_winners
            .retain(|_, family| !removed.contains(family));
        for group in &mut self.groups {
            group.members.retain(|id| !removed.contains(id));
        }
        self.groups.retain(|group| group.members.len() >= 2);
    }

    /// Records the deployment this profile's list was applied with: the enabled entries, the file winners and the
    /// time of the apply.
    pub fn mark_deployed(&mut self, mods: Vec<Entry>, winners: BTreeMap<String, String>, at: u32) {
        self.deployed_mods = mods;
        self.deployed_winners = winners;
        self.deployed_at = Some(at);
    }

    /// Forgets the deployment, so the list is a draft again. The list and the chosen winners stay.
    pub fn mark_undeployed(&mut self) {
        self.deployed_mods.clear();
        self.deployed_winners.clear();
        self.deployed_at = None;
    }

    /// Whether two profiles carry the same editable list. This is the comparison a history save needs; it borrows
    /// both lists instead of cloning them through `list_state`.
    pub fn same_list(&self, other: &Self) -> bool {
        self.mods == other.mods
            && self.separators == other.separators
            && self.groups == other.groups
            && self.winners == other.winners
            && self.waiting == other.waiting
    }

    /// The list content a user edits: what a change to the list touches and what undo puts back.
    pub fn list_state(&self) -> ListState {
        ListState {
            mods: self.mods.clone(),
            separators: self.separators.clone(),
            groups: self.groups.clone(),
            winners: self.winners.clone(),
            waiting: self.waiting.clone(),
        }
    }

    /// Puts back a list state. Entries of mods that are gone are dropped at the next status read.
    pub fn restore_list(&mut self, state: ListState) {
        self.mods = state.mods;
        self.separators = state.separators;
        self.groups = state.groups;
        self.winners = state.winners;
        self.waiting = state.waiting;
    }
}

/// The part of a profile that list edits change: the mods with their order, the separators, the groups, the file winners
/// and the pack mods still waiting for the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListState {
    pub mods: Vec<Entry>,
    #[serde(default)]
    pub separators: Vec<Separator>,
    #[serde(default)]
    pub groups: Vec<ModGroup>,
    #[serde(default)]
    pub winners: BTreeMap<String, String>,
    #[serde(default)]
    pub waiting: Vec<WaitingMod>,
}

impl Profile {
    /// How many file winners differ from the last deployment. Each one is a draft change.
    pub fn winner_changes(&self) -> u32 {
        history::winner_changes(&self.deployed_winners, &self.winners)
    }

    /// How many list changes wait for a deployment: added, removed, moved or updated mods, and file winners.
    pub fn draft_changes(&self) -> u32 {
        diff::changed_count_filtered(&self.deployed_mods, &self.mods, |entry| entry.enabled)
            + self.winner_changes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, folder: &str, enabled: bool) -> Entry {
        Entry {
            id: id.to_string(),
            folder: folder.to_string(),
            enabled,
            priority: None,
        }
    }

    fn profile() -> Profile {
        Profile::new(
            "main".to_string(),
            "Main".to_string(),
            vec![
                entry("alpha", "alpha-folder", true),
                entry("beta", "beta-folder", false),
                entry("gamma", "gamma-folder", true),
            ],
        )
    }

    #[test]
    fn enabled_folders_and_count_follow_the_list_order() {
        let profile = profile();
        assert_eq!(profile.enabled_folders(), ["alpha-folder", "gamma-folder"]);
        assert_eq!(profile.enabled_count(), 2);
        assert_eq!(
            profile.enabled_folders().len(),
            profile.enabled_mods().len()
        );
    }

    #[test]
    fn a_user_data_set_the_profile_does_not_name_is_shared() {
        let mut profile = profile();
        profile.set_user_data_isolation("saves", Isolation::Isolated);

        assert_eq!(
            profile.user_data_isolation("saves"),
            Isolation::Isolated,
            "an explicit choice is kept"
        );
        assert_eq!(
            profile.user_data_isolation("documents"),
            Isolation::Shared,
            "a set the profile predates is shared"
        );
    }

    #[test]
    fn retarget_family_moves_only_the_matching_anchors() {
        let mut profile = profile();
        let separator = |id: &str, before: Option<&str>| Separator {
            id: id.to_string(),
            name: id.to_string(),
            before: before.map(str::to_string),
        };
        profile.separators = vec![
            separator("sep1", Some("waiting-x")),
            separator("sep2", Some("alpha")),
            separator("sep3", None),
        ];

        profile.retarget_family("waiting-x", "delta");

        let anchors: Vec<Option<&str>> = profile
            .separators
            .iter()
            .map(|separator| separator.before.as_deref())
            .collect();
        assert_eq!(anchors, [Some("delta"), Some("alpha"), None]);
    }

    #[test]
    fn retarget_family_renames_winners_groups_and_waiting_anchors() {
        let mut profile = profile();
        profile.winners = BTreeMap::from([
            ("textures/a.dds".to_string(), "waiting-x".to_string()),
            ("textures/b.dds".to_string(), "alpha".to_string()),
        ]);
        profile.groups = vec![
            ModGroup {
                id: "grp1".to_string(),
                name: "Set".to_string(),
                members: vec!["alpha".to_string(), "waiting-x".to_string()],
                kind: crate::domain::profiles::GroupKind::Bundle,
                language: None,
            },
            ModGroup {
                id: "grp2".to_string(),
                name: "Both".to_string(),
                members: vec!["waiting-x".to_string(), "delta".to_string()],
                kind: crate::domain::profiles::GroupKind::Bundle,
                language: None,
            },
        ];
        let waiting = |key: &str, before: Option<&str>| WaitingMod {
            before: before.map(str::to_string),
            entry: serde_json::from_str(&format!(
                r#"{{"name":"{key}","version":null,"family":"{key}","enabled":true}}"#
            ))
            .unwrap(),
        };
        profile.waiting = vec![
            waiting("waiting-y", Some("waiting-x")),
            waiting("waiting-z", Some("alpha")),
        ];

        profile.retarget_family("waiting-x", "delta");

        assert_eq!(profile.winners["textures/a.dds"], "delta");
        assert_eq!(profile.winners["textures/b.dds"], "alpha");
        // A group that already names the target keeps one member, in the first place.
        assert_eq!(profile.groups[0].members, ["alpha", "delta"]);
        assert_eq!(profile.groups[1].members, ["delta"]);
        assert_eq!(profile.waiting[0].before.as_deref(), Some("delta"));
        assert_eq!(profile.waiting[1].before.as_deref(), Some("alpha"));
    }
}
