//! The data of a pack mod that the local copy lacks: tags, notes, instructions, relations and the provider page.

use std::collections::BTreeMap;

use crate::domain::pack::PackMod;
use crate::library::Library;

/// Copies the pack's family data into the local family where the local field is still empty. What the user has set
/// stays.
pub(super) fn adopt_family_data(
    library: &mut Library,
    family: &str,
    entry: &PackMod,
    families: &BTreeMap<&str, &str>,
) {
    let Some(local) = library.families.get_mut(family) else {
        return;
    };
    let ids = |list: &[String]| -> Vec<String> {
        list.iter()
            .map(|id| {
                families
                    .get(id.as_str())
                    .map_or_else(|| id.clone(), |local| (*local).to_string())
            })
            .collect()
    };
    if local.instructions.is_empty() {
        local.instructions.clone_from(&entry.instructions);
    }
    if local.tags.is_empty() {
        local.tags = entry.tags.iter().cloned().collect();
    }
    if local.requires.is_empty() {
        local.requires = ids(&entry.requires);
    }
    if local.requirements.is_empty() {
        local.requirements.clone_from(&entry.requirements);
    }
    for (provider, source) in &entry.sources {
        match local.sources.get_mut(provider) {
            Some(local_source) => {
                local_source.adopt_missing(source);
            }
            None => {
                local.sources.insert(provider.clone(), source.clone());
            }
        }
    }
    if local.source_dlcs.is_empty() {
        local.source_dlcs.clone_from(&entry.source_dlcs);
    }
    if local.notes.is_empty() {
        local.notes.clone_from(&entry.notes);
    }
    if local.incompatible.is_empty() {
        local.incompatible = ids(&entry.incompatible);
    }
    if local.overrides.is_empty() {
        local.overrides = ids(&entry.overrides);
    }
}
