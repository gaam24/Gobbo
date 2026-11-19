use std::collections::BTreeSet;

use super::super::profile::Profile;
use super::Profiles;

impl Profiles {
    /// `wanted`, or `wanted (2)`, `wanted (3)` ... when another profile has the name.
    pub fn free_name(&self, wanted: &str) -> String {
        let wanted = wanted.trim();
        crate::domain::naming::first_free(
            |number| crate::domain::naming::numbered_name(wanted, number),
            |name| !self.name_taken(name, None),
        )
    }

    /// The next free profile id, numbered above the current profile count.
    pub(crate) fn next_id(&self) -> String {
        // The old loop started above the profile count. Keep that start so a gap in the ids is still skipped.
        let offset = self.profiles.len() as u64;
        crate::domain::naming::first_free(
            |number| format!("p{}", u64::from(number).saturating_add(offset)),
            |id| !self.profiles.iter().any(|profile| profile.id == id),
        )
    }
}

/// The next free group id: `grp1`, `grp2`, ... above every id the profile already uses.
pub fn next_group_id(profile: &Profile) -> String {
    let used: BTreeSet<&str> = profile
        .groups
        .iter()
        .map(|group| group.id.as_str())
        .collect();
    crate::domain::naming::first_free(|number| format!("grp{number}"), |id| !used.contains(id))
}

/// The next free separator id: `sep1`, `sep2`, ... above every id the profile already uses.
pub fn next_separator_id(profile: &Profile) -> String {
    let used: BTreeSet<&str> = profile
        .separators
        .iter()
        .map(|separator| separator.id.as_str())
        .collect();
    crate::domain::naming::first_free(|number| format!("sep{number}"), |id| !used.contains(id))
}
