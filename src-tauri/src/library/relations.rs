//! The provider checks behind the status: requirements that are not met, and the warnings about relations between mods.

use crate::domain::naming::requirement_label;
use crate::domain::profiles::entry::ProfileMod;
use crate::domain::source::SourceRef;
use crate::domain::source::SourceRequirement;
use crate::error::Message;
use crate::library::ArchiveLabel;
use crate::library::Family;
use crate::library::Library;
use crate::library::LibraryView;
use crate::library::ModInfo;
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::collections::BTreeSet;

/// Where a requirement stands in the profile: the mod is not in the library, it is in the library but off, or on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RequirementState {
    Enabled,
    Disabled,
    Missing,
}

/// Without `info.json`, the name may come from a provider page, the name of an archive or the family, and the version
/// from the archive name.
pub fn with_source_name(
    mut info: ModInfo,
    folder: &str,
    family_name: Option<&str>,
    page_name: Option<&str>,
    archive: Option<&ArchiveLabel>,
) -> ModInfo {
    // A name taken from the folder or the archive file is a stand-in: a provider page's name is better. One from
    // `info.json` or from the user stays.
    let bare = info.name.is_empty() || info.name == folder;
    let label_name = archive.and_then(|archive| archive.name.as_deref());
    let stand_in = bare || label_name.is_some_and(|name| name == info.name);
    if let Some(name) = page_name.filter(|name| stand_in && !name.is_empty())
        && info.name != name
    {
        info.name = name.to_string();
    } else if bare {
        let name = label_name.or_else(|| family_name.filter(|name| !name.is_empty()));
        // The fallback is often the name the record already has; do not allocate a copy of it.
        if let Some(name) = name
            && info.name != name
        {
            info.name = name.to_string();
        }
    }
    if info.version.is_none()
        && let Some(archive) = archive
    {
        info.version = Some(archive.version.clone());
    }
    info
}

/// The families of the library by source, and the families that are on in the profile. Every requirement of the profile
/// is checked against them, so the library is read once for all the mods and not once per mod.
pub struct SourceLookup<'view, 'data, 'list> {
    view: &'view LibraryView<'data>,
    enabled: rustc_hash::FxHashSet<&'list str>,
    /// The enabled families in list order, so warnings stay deterministic even though the lookup set is hashed.
    enabled_order: Vec<&'list str>,
}

impl<'view, 'data, 'list> SourceLookup<'view, 'data, 'list> {
    pub fn new(view: &'view LibraryView<'data>, listed: &'list [ProfileMod]) -> Self {
        let mut enabled = rustc_hash::FxHashSet::default();
        let mut enabled_order = Vec::new();
        for entry in listed.iter().filter(|entry| entry.enabled) {
            if enabled.insert(entry.id.as_str()) {
                enabled_order.push(entry.id.as_str());
            }
        }
        // The old set iterated in family-id order; keep the warning order stable.
        enabled_order.sort_unstable();
        Self {
            view,
            enabled,
            enabled_order,
        }
    }

    /// Where a requirement on any provider stands. A page requirement is met by any of its parts.
    pub fn state_source(&self, source: &SourceRef) -> RequirementState {
        let mut families = self.view.families_of_page(&source.provider, &source.mod_id);
        let Some(first) = families.next() else {
            return RequirementState::Missing;
        };
        if self.enabled.contains(first) || families.any(|family| self.enabled.contains(family)) {
            RequirementState::Enabled
        } else {
            RequirementState::Disabled
        }
    }

    /// Whether the family is on in the profile. A family that is in the library but has no enabled entry is off.
    pub(crate) fn is_enabled(&self, family: &str) -> bool {
        self.enabled.contains(family)
    }
}

/// The external requirements of a mod, in lists: the ones that share a provider and a group are alternatives and make
/// one list, and each other requirement is a list of its own. The order of the requirements is kept.
pub fn source_alternatives(requirements: &[SourceRequirement]) -> Vec<Vec<&SourceRequirement>> {
    let mut lists: Vec<Vec<&SourceRequirement>> = Vec::new();
    let mut list_of_group: FxHashMap<(&str, u32), usize> = FxHashMap::default();
    for requirement in requirements {
        let Some(group) = requirement.group else {
            lists.push(vec![requirement]);
            continue;
        };
        let key = (requirement.source.provider.as_str(), group);
        let at = *list_of_group.entry(key).or_insert_with(|| {
            lists.push(Vec::new());
            lists.len() - 1
        });
        lists[at].push(requirement);
    }
    lists
}

/// The name of a family of the library, or its id when the library has no family of that id.
pub fn family_name(library: &Library, id: &str) -> String {
    library
        .families
        .get(id)
        .map_or_else(|| id.to_string(), |family| family.name.clone())
}

/// One unmet requirement of a family. The count and the messages both use this list, so the two never disagree.
enum Unmet<'a> {
    /// A mod of the library that the family needs, missing from the library.
    RequiredMissing(&'a str),
    /// A mod of the library that the family needs, switched off in the profile.
    RequiredDisabled(&'a str),
    /// A list of provider alternatives; none of them is on. The state is the one of the first alternative, which tells
    /// the message whether its mod is missing or switched off.
    Alternatives {
        alternatives: Vec<&'a SourceRequirement>,
        state: RequirementState,
    },
}

fn unmet_descriptors<'a>(
    library: &Library,
    lookup: &SourceLookup<'_, '_, '_>,
    family: &'a Family,
) -> Vec<Unmet<'a>> {
    let mut unmet = Vec::new();
    for required in &family.requires {
        if !library.families.contains_key(required) {
            unmet.push(Unmet::RequiredMissing(required));
        } else if !lookup.enabled.contains(required.as_str()) {
            unmet.push(Unmet::RequiredDisabled(required));
        }
    }
    for alternatives in source_alternatives(&family.requirements) {
        // An ignored requirement stays in the inspector but never counts: a list of ignored alternatives is skipped, and
        // an ignored one cannot meet a list another requirement still has to meet.
        let alternatives: Vec<&SourceRequirement> = alternatives
            .into_iter()
            .filter(|requirement| !requirement.ignored)
            .collect();
        if alternatives.is_empty() {
            continue;
        }
        let mut state = RequirementState::Missing;
        let mut met = false;
        for (index, requirement) in alternatives.iter().enumerate() {
            let found = lookup.state_source(&requirement.source);
            if index == 0 {
                state = found;
            }
            if found == RequirementState::Enabled {
                met = true;
                break;
            }
        }
        if met {
            continue;
        }
        unmet.push(Unmet::Alternatives {
            alternatives,
            state,
        });
    }
    unmet
}

/// The message of one unmet requirement of `family`.
fn unmet_message(library: &Library, family: &Family, unmet: &Unmet<'_>) -> Message {
    match unmet {
        Unmet::RequiredMissing(required) => Message::new("relations.requires_missing")
            .with("mod", &family.name)
            .with("requirement", required),
        Unmet::RequiredDisabled(required) => Message::new("relations.requires_disabled")
            .with("mod", &family.name)
            .with("requirement", family_name(library, required)),
        Unmet::Alternatives {
            alternatives,
            state,
        } => {
            let name_of = |requirement: &SourceRequirement| {
                requirement_label(
                    &requirement.name,
                    requirement.source.provider.as_str(),
                    &requirement.source.mod_id,
                )
            };
            let [requirement] = alternatives.as_slice() else {
                let names = alternatives
                    .iter()
                    .map(|requirement| name_of(requirement))
                    .collect::<Vec<_>>()
                    .join(" / ");
                return Message::new("relations.requires_one_of")
                    .with("mod", &family.name)
                    .with("requirement", names);
            };
            let message = match state {
                RequirementState::Missing => Message::new("relations.requires_missing"),
                _ => Message::new("relations.requires_disabled"),
            };
            message
                .with("mod", &family.name)
                .with("requirement", name_of(requirement))
        }
    }
}

/// The unmet requirements of every family in a profile, built once for the rows and the warnings of one status.
pub struct UnmetRequirements<'a> {
    library: &'a Library,
    by_family: FxHashMap<&'a str, Vec<Unmet<'a>>>,
}

impl<'a> UnmetRequirements<'a> {
    /// Builds the descriptors of the families in `listed`, each family once. A family without requirements needs no
    /// entry.
    pub fn of(
        library: &'a Library,
        lookup: &SourceLookup<'_, '_, '_>,
        listed: &[ProfileMod],
    ) -> Self {
        let mut by_family = FxHashMap::default();
        for entry in listed {
            let Some(family) = library.families.get(&entry.id) else {
                continue;
            };
            if family.requires.is_empty() && family.requirements.is_empty() {
                continue;
            }
            by_family
                .entry(family.id.as_str())
                .or_insert_with(|| unmet_descriptors(library, lookup, family));
        }
        Self { library, by_family }
    }

    /// The number of requirements of `family` that the profile does not meet.
    pub fn count(&self, family: &str) -> u32 {
        u32::try_from(self.by_family.get(family).map_or(0, Vec::len)).unwrap_or(u32::MAX)
    }

    /// The messages of the unmet requirements of `family`, one per requirement.
    pub fn messages(&self, family: &Family) -> Vec<Message> {
        self.by_family
            .get(family.id.as_str())
            .map_or_else(Vec::new, |unmet| {
                unmet
                    .iter()
                    .map(|unmet| unmet_message(self.library, family, unmet))
                    .collect()
            })
    }
}

/// Warnings about the enabled mods of the profile: a required mod that is not in the library or is switched off (one of
/// several alternatives is enough), two enabled mods that do not work together, and a mod below a mod it should override.
/// Each pair is reported once.
pub fn relation_warnings(
    library: &Library,
    view: &LibraryView<'_>,
    listed: &[ProfileMod],
) -> Vec<Message> {
    let lookup = SourceLookup::new(view, listed);
    let unmet = UnmetRequirements::of(library, &lookup, listed);
    relation_warnings_with(library, &lookup, listed, &unmet)
}

/// `relation_warnings` with an index and the unmet requirements the caller already built, so a status does not read the
/// whole library twice and does not build the requirement descriptors twice.
pub fn relation_warnings_with(
    library: &Library,
    lookup: &SourceLookup<'_, '_, '_>,
    listed: &[ProfileMod],
    unmet: &UnmetRequirements<'_>,
) -> Vec<Message> {
    let enabled = &lookup.enabled;
    // Most families override nothing. Building the position index only when one does saves a map and a pass over
    // the whole list on every status.
    let mut position: Option<FxHashMap<&str, usize>> = None;
    let mut warnings = Vec::new();
    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for id in &lookup.enabled_order {
        let Some(family) = library.families.get(*id) else {
            continue;
        };
        warnings.extend(unmet.messages(family));
        for other in &family.overrides {
            let position = position.get_or_insert_with(|| {
                listed
                    .iter()
                    .enumerate()
                    .map(|(at, entry)| (entry.id.as_str(), at))
                    .collect()
            });
            if enabled.contains(other.as_str()) && position.get(other.as_str()) < position.get(*id)
            {
                warnings.push(
                    Message::new("relations.order")
                        .with("mod", &family.name)
                        .with("other", family_name(library, other)),
                );
            }
        }
        for other in &family.incompatible {
            if enabled.contains(other.as_str()) {
                let pair = if *id <= other.as_str() {
                    ((*id).to_string(), other.clone())
                } else {
                    (other.clone(), (*id).to_string())
                };
                if pairs.insert(pair.clone()) {
                    warnings.push(
                        Message::new("relations.conflict")
                            .with("first", family_name(library, &pair.0))
                            .with("second", family_name(library, &pair.1)),
                    );
                }
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str) -> ModInfo {
        ModInfo {
            name: name.to_string(),
            ..ModInfo::default()
        }
    }

    fn archive(name: &str) -> ArchiveLabel {
        ArchiveLabel {
            name: Some(name.to_string()),
            version: "1.2".to_string(),
        }
    }

    #[test]
    fn a_stand_in_name_gives_way_to_the_nexus_page_name() {
        let page = Some("Page Name");
        let arch = archive("Archive Name");
        assert_eq!(
            with_source_name(info(""), "folder", None, page, Some(&arch)).name,
            "Page Name"
        );
        assert_eq!(
            with_source_name(info("folder"), "folder", None, page, None).name,
            "Page Name"
        );
        assert_eq!(
            with_source_name(info("Archive Name"), "folder", None, page, Some(&arch)).name,
            "Page Name"
        );
        assert_eq!(
            with_source_name(info("Own Name"), "folder", None, page, Some(&arch)).name,
            "Own Name"
        );
    }

    #[test]
    fn without_a_page_name_a_bare_mod_takes_the_archive_then_the_family_name() {
        let arch = archive("Archive Name");
        assert_eq!(
            with_source_name(info(""), "folder", None, Some(""), Some(&arch)).name,
            "Archive Name"
        );
        assert_eq!(
            with_source_name(info(""), "folder", Some("Family"), None, Some(&arch)).name,
            "Archive Name"
        );
        assert_eq!(
            with_source_name(info(""), "folder", Some("Family"), None, None).name,
            "Family"
        );
        assert_eq!(
            with_source_name(info(""), "folder", Some(""), None, None).name,
            ""
        );
        assert_eq!(
            with_source_name(info("Own Name"), "folder", Some("Family"), None, None).name,
            "Own Name"
        );
    }

    #[test]
    fn the_row_counts_add_up_to_the_requirement_warnings() {
        let mut library = Library::default();
        library.families.insert(
            "nexus:1".into(),
            Family {
                id: "nexus:1".into(),
                name: "One".into(),
                requires: vec!["nexus:2".into(), "nexus:3".into()],
                requirements: vec![SourceRequirement {
                    source: SourceRef::new("nexus", "9"),
                    name: "Nine".into(),
                    from_source: false,
                    group: None,
                    ignored: false,
                }],
                ..Family::default()
            },
        );
        library.families.insert(
            "nexus:2".into(),
            Family {
                id: "nexus:2".into(),
                name: "Two".into(),
                ..Family::default()
            },
        );
        // nexus:2 is in the library but off; nexus:3 is missing from it.
        let listed = vec![
            crate::test_support::profile_mod("nexus:1", "", true),
            crate::test_support::profile_mod("nexus:2", "", false),
        ];
        let view = LibraryView::new(&library);
        let lookup = SourceLookup::new(&view, &listed);
        let unmet = UnmetRequirements::of(&library, &lookup, &listed);

        let counted: u32 = listed.iter().map(|entry| unmet.count(&entry.id)).sum();
        let warnings = relation_warnings_with(&library, &lookup, &listed, &unmet);
        assert_eq!(counted, 3);
        assert_eq!(warnings.len() as u32, counted);
        assert_eq!(
            warnings
                .iter()
                .map(|warning| warning.code.as_str())
                .collect::<Vec<_>>(),
            [
                "relations.requires_disabled",
                "relations.requires_missing",
                "relations.requires_missing",
            ]
        );
    }
}
