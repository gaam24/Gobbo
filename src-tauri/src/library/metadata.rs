//! The metadata a user edits on a mod: tags, name and version, note, relations, instructions and file winners.

use crate::context::Data;
use crate::context::Loaded;
use crate::context::library::{mod_info, not_in_library};
use crate::context::load_game;
use crate::context::load_profiles;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::context::{edit_family, edit_library};
use crate::domain::library::Family;
use crate::domain::library::ModInfoEdit;
use crate::domain::library::{Library, LibraryView};
use crate::domain::naming::requirement_label;
use crate::domain::source::{SourceRef, SourceRequirement};
use crate::error::AppError;
use serde::Deserialize;
use serde::Serialize;

/// Saves the family's tags. Empty ones and ones trimmed of spaces are dropped, the rest go into the index.
pub fn set_mod_tags(
    root: &Data,
    game_id: &str,
    id: &str,
    tags: Vec<String>,
) -> Result<(), AppError> {
    edit_family(root, game_id, id, |family| {
        family.tags = tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
        Ok(())
    })
}

/// Saves the name, version and author the user typed for one version of a mod. They win over `info.json`, and an empty
/// field goes back to the value from the file.
pub fn set_mod_info(
    root: &Data,
    game_id: &str,
    id: &str,
    folder: &str,
    edit: ModInfoEdit,
) -> Result<(), AppError> {
    edit_library(root, game_id, |data, _, library| {
        let version = library.version_of_family_mut(folder, id)?;
        let edit = edit.cleaned();
        let mut info = mod_info(&data.library().join(folder), folder);
        edit.apply(&mut info);
        version.info = info;
        version.edit = edit;
        Ok(())
    })
}

/// Saves the user's note about the family.
pub fn set_mod_note(root: &Data, game_id: &str, id: &str, note: String) -> Result<(), AppError> {
    edit_family(root, game_id, id, |family| {
        family.notes = note;
        Ok(())
    })
}

/// Saves the mods a mod requires and the mods it does not work with. Each must be another mod in the library.
pub fn set_mod_relations(
    root: &Data,
    game_id: &str,
    id: &str,
    relations: &Relations,
) -> Result<(), AppError> {
    edit_library(root, game_id, |_, _, library| {
        if !library.families.contains_key(id) {
            return Err(not_in_library(id));
        }

        let requires = checked_relations(library, id, &relations.requires)?;
        let requirements = checked_requirements(library, id, &relations.requirements)?;
        let incompatible = checked_relations(library, id, &relations.incompatible)?;
        let overrides = checked_relations(library, id, &relations.overrides)?;
        let family = library.family_mut(id)?;
        family.requires = requires;
        family.requirements = requirements;
        family.incompatible = incompatible;
        family.overrides = overrides;
        Ok(())
    })
}

/// The relations of a mod to other mods (family ids), and to mods on external providers that may not be in the
/// library.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
pub struct Relations {
    /// Mods it needs to work.
    pub requires: Vec<String>,
    /// Mods on external providers it needs to work, which may not be in the library.
    #[serde(default)]
    pub requirements: Vec<SourceRequirement>,
    /// Mods it does not work with.
    pub incompatible: Vec<String>,
    /// Mods it overrides: it stands above them in the list, so its copy of a shared file wins.
    pub overrides: Vec<String>,
}

/// Whether two requirements name the same provider page: the same provider and mod id. The file id is deliberately
/// excluded, because any file of the page satisfies a requirement. This answers the opposite question from
/// `crate::domain::pack::PackMod::is_same_mod`, where the exact file decides, and is not the file question answered by
/// `crate::domain::source::SourceRef::same_file`.
fn same_page(left: &SourceRef, right: &SourceRef) -> bool {
    left.provider == right.provider && left.mod_id == right.mod_id
}

/// The external requirements a mod needs, without repeats. A listed requirement stays as it is; a typed one is checked.
fn checked_requirements(
    library: &Library,
    id: &str,
    requirements: &[SourceRequirement],
) -> Result<Vec<SourceRequirement>, AppError> {
    let saved = library
        .families
        .get(id)
        .map_or_else(Vec::new, |family| family.requirements.clone());
    let mut kept: Vec<SourceRequirement> = Vec::new();

    for requirement in requirements {
        let source = &requirement.source;

        if let Some(listed) = saved
            .iter()
            .find(|listed| listed.from_source && same_page(&listed.source, source))
        {
            if !kept.iter().any(|other| same_page(&other.source, source)) {
                let mut kept_requirement = listed.clone();
                kept_requirement.ignored = requirement.ignored;
                kept.push(kept_requirement);
            }

            continue;
        }

        if kept.iter().any(|other| same_page(&other.source, source)) {
            continue;
        }

        kept.push(checked_requirement(library, id, requirement)?);
    }

    Ok(kept)
}

/// One typed requirement, checked: the provider must exist, the id must not be empty, the mod must not name the mod
/// itself and must not already be in the library. The result is not from the provider, and its name is filled in.
fn checked_requirement(
    library: &Library,
    id: &str,
    requirement: &SourceRequirement,
) -> Result<SourceRequirement, AppError> {
    let view = LibraryView::new(library);
    let source = &requirement.source;
    let own_sources = library
        .families
        .get(id)
        .map(|family| family.sources.clone())
        .unwrap_or_default();
    let Some(provider) = crate::providers::provider(source.provider.as_str()) else {
        return Err(
            AppError::user("relations.source_unknown").with("provider", source.provider.as_str())
        );
    };

    if !provider.valid_mod_id(&source.mod_id)
        || own_sources
            .get(&source.provider)
            .is_some_and(|own| own.mod_id == source.mod_id)
    {
        return Err(AppError::user("relations.source_bad_id")
            .with("provider", source.provider.as_str())
            .with("mod", &source.mod_id));
    }

    if view
        .family_of_source(&source.provider, &source.mod_id)
        .is_some()
    {
        return Err(AppError::user("relations.source_in_library")
            .with("provider", source.provider.as_str())
            .with("mod", &source.mod_id));
    }

    let mut typed = requirement.clone();
    typed.from_source = false;
    typed.group = None;
    let name = typed.name.trim().to_string();
    typed.name = requirement_label(&name, provider.display_name(), &source.mod_id);
    Ok(typed)
}

/// Adds one requirement to a mod. A requirement for a mod already listed (same provider and mod id) changes nothing:
/// the listed one keeps its name and its switch.
pub fn add_requirement(
    root: &Data,
    game_id: &str,
    id: &str,
    requirement: &SourceRequirement,
) -> Result<(), AppError> {
    edit_library(root, game_id, |_, _, library| {
        if !library.families.contains_key(id) {
            return Err(not_in_library(id));
        }

        let listed = library.families.get(id).is_some_and(|family| {
            family
                .requirements
                .iter()
                .any(|listed| same_page(&listed.source, &requirement.source))
        });

        if listed {
            return Ok(());
        }

        let typed = checked_requirement(library, id, requirement)?;
        library.family_mut(id)?.requirements.push(typed);
        Ok(())
    })
}

/// Removes the requirement for one provider mod from a mod. A mod that is not listed changes nothing.
pub fn remove_requirement(
    root: &Data,
    game_id: &str,
    id: &str,
    provider: &str,
    req_mod_id: &str,
) -> Result<(), AppError> {
    edit_family(root, game_id, id, |family| {
        family
            .requirements
            .retain(|listed| !is_requirement(listed, provider, req_mod_id));
        Ok(())
    })
}

/// Sets whether the requirement for one provider mod is ignored. A mod that is not listed changes nothing.
pub fn set_requirement_ignored(
    root: &Data,
    game_id: &str,
    id: &str,
    provider: &str,
    req_mod_id: &str,
    ignored: bool,
) -> Result<(), AppError> {
    edit_family(root, game_id, id, |family| {
        for listed in family
            .requirements
            .iter_mut()
            .filter(|listed| is_requirement(listed, provider, req_mod_id))
        {
            listed.ignored = ignored;
        }

        Ok(())
    })
}

/// Whether a requirement names the mod with this provider and mod id.
fn is_requirement(requirement: &SourceRequirement, provider: &str, req_mod_id: &str) -> bool {
    requirement.source.provider.as_str() == provider && requirement.source.mod_id == req_mod_id
}

/// One other mod of the library, named by its family id. It must exist and must not be the mod itself.
fn checked_relation(library: &Library, id: &str, other: &str) -> Result<String, AppError> {
    if other == id || !library.families.contains_key(other) {
        return Err(not_in_library(other));
    }

    Ok(other.to_string())
}

/// The ids that name other mods of the library, without repeats, in the order given.
fn checked_relations(library: &Library, id: &str, ids: &[String]) -> Result<Vec<String>, AppError> {
    let mut kept: Vec<String> = Vec::new();

    for other in ids {
        let other = checked_relation(library, id, other)?;

        if !kept.contains(&other) {
            kept.push(other);
        }
    }

    Ok(kept)
}

/// The lists of other mods a mod relates to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// Mods it needs to work.
    Requires,
    /// Mods it does not work with.
    Incompatible,
    /// Mods it stands above in the list, so its copy of a shared file wins.
    Overrides,
}

impl RelationKind {
    /// The list this kind edits on a family.
    const fn list(self, family: &mut Family) -> &mut Vec<String> {
        match self {
            Self::Requires => &mut family.requires,
            Self::Incompatible => &mut family.incompatible,
            Self::Overrides => &mut family.overrides,
        }
    }
}

/// Adds one mod to one of a mod's relation lists. A relation already listed changes nothing. The other mod must be in
/// the library and must not be the mod itself. The edit merges with the index as it is read, so two quick edits keep
/// both.
pub fn add_relation(
    root: &Data,
    game_id: &str,
    id: &str,
    kind: RelationKind,
    other: &str,
) -> Result<(), AppError> {
    edit_library(root, game_id, |_, _, library| {
        if !library.families.contains_key(id) {
            return Err(not_in_library(id));
        }

        let other = checked_relation(library, id, other)?;
        let list = kind.list(library.family_mut(id)?);

        if !list.contains(&other) {
            list.push(other);
        }

        Ok(())
    })
}

/// Removes one mod from one of a mod's relation lists. A mod that is not listed changes nothing.
pub fn remove_relation(
    root: &Data,
    game_id: &str,
    id: &str,
    kind: RelationKind,
    other: &str,
) -> Result<(), AppError> {
    edit_family(root, game_id, id, |family| {
        kind.list(family).retain(|listed| listed != other);
        Ok(())
    })
}

/// Chooses which mod's copy of a file reaches the game, in the active profile. `None` goes back to the list order.
/// The file is named the way the conflict list names it, so its key matches the plan's.
pub fn set_file_winner(
    root: &Data,
    game_id: &str,
    path: &str,
    family: Option<&str>,
) -> Result<(), AppError> {
    let Loaded { data, library, .. } = load_game(root, game_id)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let winners = &mut profiles.active_mut().winners;

    match family {
        Some(id) => {
            if !library.families.contains_key(id) {
                return Err(not_in_library(id));
            }

            winners.insert(crate::domain::pathkey::path_key(path), id.to_string());
        }
        None => {
            winners.remove(&crate::domain::pathkey::path_key(path));
        }
    }

    save_profiles(&data, &profiles)
}

/// Saves the instructions for players. Surrounding spaces are dropped, and an empty text removes them.
pub fn set_mod_instructions(
    root: &Data,
    game_id: &str,
    id: &str,
    text: &str,
) -> Result<(), AppError> {
    edit_family(root, game_id, id, |family| {
        family.instructions = text.trim().to_string();
        Ok(())
    })
}
