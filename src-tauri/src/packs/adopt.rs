//! The data of a pack mod that the local copy lacks: tags, notes, instructions, relations and the provider page.

use std::collections::BTreeMap;

use crate::domain::library::Library;
use crate::domain::library::{AdoptPolicy, Family, adopt_fields};
use crate::domain::pack::PackMod;

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
    let source = Family {
        tags: entry.tags.iter().cloned().collect(),
        notes: entry.notes.clone(),
        instructions: entry.instructions.clone(),
        requires: entry.requires.clone(),
        requirements: entry.requirements.clone(),
        source_dlcs: entry.source_dlcs.clone(),
        incompatible: entry.incompatible.clone(),
        overrides: entry.overrides.clone(),
        ..Family::default()
    };
    adopt_fields(local, &source, AdoptPolicy::FillEmpty, |id| {
        families
            .get(id)
            .map_or_else(|| id.to_string(), |local| (*local).to_string())
    });

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
}
