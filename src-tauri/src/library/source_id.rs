//! The external source id of a mod family. The id the user types can move the family to the `<provider>:<id>` family,
//! which takes over the tags, notes, requirements and profile places of the family before it.

use crate::context::profile::lock_profiles;
use crate::context::{Data, edit_library};
use crate::domain::source::{ProviderId, SourceMod};
use crate::error::AppError;
use crate::library::{Family, Library, not_in_library};

/// Saves the source id the user typed for a family. `None` removes that provider's entry, so the id from the archive
/// name applies again if the family has one.
pub fn set_mod_source(
    root: &Data,
    game_id: &str,
    id: &str,
    provider_id: &str,
    mod_id: Option<String>,
) -> Result<(), AppError> {
    let provider = crate::providers::require(provider_id)?;
    let mod_id = mod_id.map(|mod_id| mod_id.trim().to_string());

    if let Some(mod_id) = &mod_id
        && !provider.valid_mod_id(mod_id)
    {
        return Err(AppError::user("source.bad_id")
            .with("provider", provider_id)
            .with("id", mod_id));
    }

    edit_library(root, game_id, |data, _, library| {
        let provider = ProviderId::new(provider_id);
        let family = library.family_mut(id)?;
        let Some(mod_id) = mod_id else {
            family.clear_source(&provider);
            return Ok(());
        };
        let target = format!("{provider}:{mod_id}");
        // Typing the page id an addon already belongs to must not merge it into another part of that page.
        let same_page = family
            .source(provider_id)
            .is_some_and(|source| source.mod_id == mod_id);
        if same_page || target == id {
            let meta = kept_meta(family, &provider, &mod_id);
            set_source(family, &provider, &mod_id, meta);
            return Ok(());
        }

        move_family(data, library, id, &target, &provider, &mod_id)
    })
}

/// Moves the family `id` to the `<provider>:<id>` family `target`: into it when the library has it, or renamed to it.
/// The versions, the relations of other mods and the profile places follow.
fn move_family(
    data: &Data,
    library: &mut Library,
    id: &str,
    target: &str,
    provider: &ProviderId,
    mod_id: &str,
) -> Result<(), AppError> {
    let _profiles = lock_profiles();
    let mut profiles = crate::context::load_profiles(data)?;
    let mut source = library
        .families
        .remove(id)
        .ok_or_else(|| not_in_library(id))?;
    if let Some(family) = library.families.get_mut(target) {
        merge_source(family, provider, mod_id, &source);
        absorb(family, source);
    } else {
        source.id = target.to_string();
        let meta = kept_meta(&source, provider, mod_id);
        set_source(&mut source, provider, mod_id, meta);
        library.families.insert(target.to_string(), source);
    }

    for version in library
        .versions
        .values_mut()
        .filter(|version| version.family == id)
    {
        version.family = target.to_string();
    }

    library.rename_family_references(id, target);
    profiles.rename_family(id, target);
    data.save_profiles(&profiles)
}

/// Takes over the user's data of `source` into `family`: tags, a note and instructions it lacks, and the relations and
/// provider data it does not have yet.
fn absorb(family: &mut Family, source: Family) {
    family.tags.extend(source.tags);

    if family.notes.is_empty() {
        family.notes = source.notes;
    }

    if family.instructions.is_empty() {
        family.instructions = source.instructions;
    }

    for other in source.requires {
        if !family.requires.contains(&other) {
            family.requires.push(other);
        }
    }

    for other in source.incompatible {
        if !family.incompatible.contains(&other) {
            family.incompatible.push(other);
        }
    }

    for requirement in source.requirements {
        if !family
            .requirements
            .iter()
            .any(|kept| kept.source == requirement.source)
        {
            family.requirements.push(requirement);
        }
    }

    for (provider, dlcs) in source.source_dlcs {
        family.source_dlcs.entry(provider).or_insert(dlcs);
    }

    for (provider, sync) in source.source_sync {
        family.source_sync.entry(provider).or_insert(sync);
    }

    for (provider, latest) in source.latest {
        family.latest.entry(provider).or_insert(latest);
    }

    for (provider, latest_file) in source.latest_file_ids {
        family
            .latest_file_ids
            .entry(provider)
            .or_insert(latest_file);
    }

    for (provider, ignored) in source.ignored_versions {
        family.ignored_versions.entry(provider).or_insert(ignored);
    }
}

/// The page data and the last check of a source that is given the id `mod_id`. They describe one mod, so they are kept
/// only when the id stays the same; a new id is another mod, and the old page or check must not show for it.
fn kept_meta(
    family: &Family,
    provider: &ProviderId,
    mod_id: &str,
) -> Option<crate::domain::source::SourceMeta> {
    family
        .source(provider.as_str())
        .filter(|source| source.mod_id == mod_id)
        .and_then(|source| source.meta.clone())
}

fn set_source(
    family: &mut Family,
    provider: &ProviderId,
    mod_id: &str,
    meta: Option<crate::domain::source::SourceMeta>,
) {
    if family
        .sources
        .get(provider)
        .is_some_and(|entry| entry.mod_id != mod_id)
    {
        family.forget_provider(provider);
    }

    let entry = family.sources.entry(provider.clone()).or_default();

    if entry.mod_id != mod_id {
        entry.set_part_fields(None, None, None);
    }

    entry.mod_id = mod_id.to_string();
    entry.meta = meta;
}

/// Copies the sources of the family being replaced into the target, without overwriting the target's page data. The
/// typed provider's page and check are copied only when they belong to the id the user typed.
fn merge_source(family: &mut Family, provider: &ProviderId, mod_id: &str, source: &Family) {
    for (other_provider, other_source) in &source.sources {
        let typed = other_provider == provider;
        let meta = if typed && other_source.mod_id != mod_id {
            None
        } else {
            other_source.meta.clone()
        };

        if let Some(entry) = family.sources.get_mut(other_provider) {
            if typed {
                if entry.mod_id != mod_id {
                    entry.set_part_fields(None, None, None);
                }

                entry.mod_id = mod_id.to_string();
            }

            if entry.meta.is_none() {
                entry.meta = meta;
            }
        } else {
            let mut entry = SourceMod {
                mod_id: if typed {
                    mod_id.to_string()
                } else {
                    other_source.mod_id.clone()
                },
                meta,
                ..SourceMod::default()
            };

            if !typed {
                entry.set_part_fields(
                    other_source.part_id.as_deref(),
                    other_source.part_name.as_deref(),
                    other_source.part_category.as_deref(),
                );
            }

            family.sources.insert(other_provider.clone(), entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::source::{SourceMeta, SourceRequirement, SourceSync};
    use std::collections::BTreeMap;

    fn page(version: &str) -> SourceMeta {
        SourceMeta {
            version: Some(version.into()),
            checked_at: Some(1),
            ..SourceMeta::default()
        }
    }

    fn family_with(provider: &str, mod_id: &str, meta: Option<SourceMeta>) -> Family {
        Family {
            id: format!("{provider}:{mod_id}"),
            name: "Mod".into(),
            sources: BTreeMap::from([(
                ProviderId::new(provider),
                SourceMod {
                    mod_id: mod_id.into(),
                    part_id: None,
                    part_name: None,
                    part_category: None,
                    meta,
                },
            )]),
            ..Family::default()
        }
    }

    #[test]
    fn a_new_id_drops_the_page_and_the_check_of_the_old_mod() {
        let nexus = ProviderId::new("nexus");
        let mut family = family_with("nexus", "5", Some(page("2.0")));
        let kept = kept_meta(&family, &nexus, "5");
        set_source(&mut family, &nexus, "5", kept);
        assert!(
            family
                .source("nexus")
                .is_some_and(|source| source.meta.is_some())
        );

        let meta = kept_meta(&family, &nexus, "6");
        set_source(&mut family, &nexus, "6", meta);
        let source = family.source("nexus").expect("the source stays");
        assert_eq!(source.mod_id, "6");
        assert!(source.meta.is_none());
    }

    #[test]
    fn a_new_id_forgets_the_sync_and_the_listed_requirements_of_the_old_page() {
        let nexus = ProviderId::new("nexus");
        let mut family = family_with("nexus", "5", Some(page("2.0")));
        family.source_sync.insert(
            nexus.clone(),
            SourceSync {
                file_id: "7".into(),
                version_id: "v".into(),
                checked_at: 1,
                mod_level: true,
                older_version: None,
            },
        );
        family
            .source_dlcs
            .insert(nexus.clone(), vec![vec!["Extra".into()]]);
        let listed = SourceRequirement {
            source: crate::domain::source::SourceRef::new("nexus", "10"),
            name: "Listed".into(),
            from_source: true,
            group: None,
            ignored: false,
        };
        let typed = SourceRequirement {
            from_source: false,
            name: "Typed".into(),
            ..listed.clone()
        };
        family.requirements = vec![listed, typed];

        let meta = kept_meta(&family, &nexus, "6");
        set_source(&mut family, &nexus, "6", meta);

        assert!(
            family.source_sync.is_empty(),
            "the sync was read from the old page"
        );
        assert!(
            family.source_dlcs.is_empty(),
            "the DLCs were listed by the old page"
        );
        let names: Vec<&str> = family
            .requirements
            .iter()
            .map(|requirement| requirement.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["Typed"],
            "the typed requirement is the user's and stays"
        );
    }

    #[test]
    fn moving_into_a_family_that_exists_keeps_only_the_page_of_the_typed_mod() {
        let nexus = ProviderId::new("nexus");
        // The archive made family 5, the user types 7 for it, and family nexus:7 exists without a page.
        let source = family_with("nexus", "5", Some(page("2.0")));
        let mut target = family_with("nexus", "7", None);
        merge_source(&mut target, &nexus, "7", &source);
        let merged = target.source("nexus").expect("the source is there");
        assert_eq!(merged.mod_id, "7");
        assert!(merged.meta.is_none());
    }

    #[test]
    fn moving_keeps_the_page_when_it_belongs_to_the_typed_mod() {
        let nexus = ProviderId::new("nexus");
        let source = family_with("nexus", "7", Some(page("2.0")));
        let mut target = family_with("nexus", "7", None);
        merge_source(&mut target, &nexus, "7", &source);
        assert!(
            target
                .source("nexus")
                .and_then(|source| source.meta.as_ref())
                .is_some()
        );
    }

    /// Clearing the source the user typed must not hide the data a refresh read from it: the last check, the DLCs and
    /// the requirements listed by the page stay until a new check replaces them.
    #[test]
    fn clearing_the_source_keeps_the_check_and_the_listed_requirements() {
        use crate::context::load_library;
        use crate::domain::source::SourceRef;
        use crate::domain::version::IgnoredVersion;
        use crate::test_support::games as test_games;
        use crate::test_support::scratch;

        let provider = ProviderId::new("nexus");
        let listed = SourceRequirement {
            source: SourceRef::new("nexus", "22"),
            name: "Listed".into(),
            from_source: true,
            group: None,
            ignored: false,
        };
        let typed = SourceRequirement {
            from_source: false,
            name: "Typed".into(),
            ..listed.clone()
        };
        let sync = SourceSync {
            file_id: "9".into(),
            version_id: "v".into(),
            checked_at: 1,
            mod_level: true,
            older_version: None,
        };

        let mut family = family_with("nexus", "11", Some(page("2.0")));
        family.latest.insert(provider.clone(), "2.0".into());
        family
            .latest_file_ids
            .insert(provider.clone(), "file-11".into());
        family.ignored_versions.insert(
            provider.clone(),
            IgnoredVersion {
                version: "1.0".into(),
                file_id: None,
            },
        );
        family.source_sync.insert(provider.clone(), sync.clone());
        family
            .source_dlcs
            .insert(provider.clone(), vec![vec!["Extra".into()]]);
        family.requirements = vec![listed.clone(), typed.clone()];

        test_games::install();
        let root = Data::new(scratch!("source-clear-keeps-check"));
        let data = root.game(test_games::MIRROR_ID);
        let game = test_games::mirror();
        let mut library = Library::default();
        library.families.insert(family.id.clone(), family);
        library.save(&data.index()).unwrap();

        set_mod_source(&root, test_games::MIRROR_ID, "nexus:11", "nexus", None).unwrap();

        let saved = load_library(&data, &game).unwrap();
        let family = &saved.families["nexus:11"];
        assert!(family.source("nexus").is_none(), "the source is removed");
        assert!(family.latest.is_empty(), "the newest version is removed");
        assert!(
            family.latest_file_ids.is_empty(),
            "the newest file id is removed"
        );
        assert!(
            family.ignored_versions.is_empty(),
            "the ignored version is removed"
        );
        assert_eq!(
            family.source_sync.get(&provider),
            Some(&sync),
            "the last check survives"
        );
        assert_eq!(
            family.source_dlcs.get(&provider),
            Some(&vec![vec!["Extra".into()]]),
            "the listed DLCs survive"
        );
        assert_eq!(
            family.requirements,
            vec![listed, typed],
            "both the listed and the typed requirement survive"
        );
    }
}
