//! Merging a preset under a game's own rules, and the precedence rules each field follows.

use std::collections::{BTreeMap, BTreeSet};

use super::super::definition::{GameDef, LoaderPackage, Requirement, ResolvedLoader};
use super::load_any;
use crate::domain::deploy::Deploy;
use crate::domain::pathkey::path_key;

/// Merges the loader named by `game.loader` under the game's own rules, then finishes the game's `requires`: every
/// requirement path is required, with or without a loader. This is the one place a game's rules are finalized, for
/// shipped and user definitions alike. The folders are searched in order, so a user preset can replace a bundled one
/// with the same id.
pub(in crate::games) fn apply(
    game: &mut GameDef,
    dirs: &[std::path::PathBuf],
) -> Result<(), String> {
    let loaded = match game.loader.as_ref() {
        Some(reference) => {
            let loader = load_any(dirs, reference.id())?;
            let rules = loader.rules(reference.variant())?;
            let variant = reference
                .variant()
                .map(str::to_string)
                .or(loader.default_variant);
            let default_variant = reference.variant().is_none() && variant.is_some();
            let resolved = ResolvedLoader {
                id: loader.id,
                name: loader.name,
                config_mirror: rules.config_mirror.clone(),
                variant,
                default_variant,
            };
            Some((rules, resolved))
        }
        None => None,
    };

    if let Some((rules, resolved)) = loaded {
        rules.apply_to(game);
        game.resolved_loader = Some(resolved);
    }

    game.requires = with_requirement_paths(std::mem::take(&mut game.requires), &game.requirements);
    Ok(())
}

/// The `requires` that a set of requirements implies: each requirement's path is added after `requires`, dropping a
/// path whose key is already there. A requirement names a file the setup check shows, so the file is required too.
pub(in crate::games) fn with_requirement_paths(
    requires: Vec<String>,
    requirements: &[Requirement],
) -> Vec<String> {
    let paths = requirements
        .iter()
        .map(|requirement| requirement.path.clone())
        .collect();
    merge_lists(requires, paths)
}

/// Appends `over` to `base`, dropping a value whose path key is already there. The order is base first, then over.
pub(in crate::games) fn merge_lists(mut base: Vec<String>, over: Vec<String>) -> Vec<String> {
    let mut keys: BTreeSet<String> = base.iter().map(|value| path_key(value)).collect();

    for value in over {
        if keys.insert(path_key(&value)) {
            base.push(value);
        }
    }

    base
}

/// Merges the loader package. A higher layer adds check paths and `needed_by` folders; a value it sets (name, version,
/// folder, or its own build's `url` with `sha256` and `size`) replaces the lower one.
pub(super) fn merge_installs(
    base: Option<LoaderPackage>,
    over: Option<LoaderPackage>,
) -> Option<LoaderPackage> {
    match (base, over) {
        (Some(mut base), Some(over)) => {
            let replace = |base: &mut String, over: String| {
                if !over.trim().is_empty() {
                    *base = over;
                }
            };
            replace(&mut base.name, over.name);
            replace(&mut base.version, over.version);
            replace(&mut base.to, over.to);

            if !over.url.trim().is_empty() {
                base.url = over.url;
                base.sha256 = over.sha256;
                base.size = over.size;
            }

            base.check.extend(over.check);
            base.needed_by.extend(over.needed_by);
            Some(base)
        }
        (base, over) => over.or(base),
    }
}

/// Merges requirement metadata by path: a game entry replaces the loader entry for the same file and keeps the order.
pub(super) fn merge_requirements(
    mut base: Vec<Requirement>,
    over: Vec<Requirement>,
) -> Vec<Requirement> {
    for requirement in over {
        let key = path_key(&requirement.path);

        if let Some(existing) = base
            .iter_mut()
            .find(|existing| path_key(&existing.path) == key)
        {
            *existing = requirement;
        } else {
            base.push(requirement);
        }
    }

    base
}

/// Merges `over` over `base`: a slot, mirror or alias in `over` replaces the base entry with the same key, and the
/// other collections are additive.
pub(super) fn merge_deploy(base: Deploy, over: Deploy) -> Deploy {
    Deploy {
        root: over.root.or(base.root),
        root_linux: over.root_linux.or(base.root_linux),
        slots: merge_by(base.slots, over.slots, |slot| path_key(&slot.dir)),
        mirror: merge_by(base.mirror, over.mirror, |mirror| path_key(&mirror.dir)),
        root_files: merge_lists(base.root_files, over.root_files),
        resource_names: over.resource_names || base.resource_names,
        shared: {
            let mut shared = base.shared;
            shared.extend(over.shared);
            shared
        },
        protected: merge_lists(base.protected, over.protected),
        aliases: merge_aliases(base.aliases, over.aliases),
        full_root: base.full_root || over.full_root,
        moves: {
            let mut moves = base.moves;
            moves.extend(over.moves);
            moves
        },
    }
}

/// Replaces an entry with the same key and keeps the order of the base, appending a new entry.
fn merge_by<T>(mut base: Vec<T>, over: Vec<T>, key: impl Fn(&T) -> String) -> Vec<T> {
    for item in over {
        let item_key = key(&item);

        if let Some(existing) = base.iter_mut().find(|existing| key(existing) == item_key) {
            *existing = item;
        } else {
            base.push(item);
        }
    }

    base
}

/// Replaces an alias with the same name and keeps the map's order.
fn merge_aliases(
    base: BTreeMap<String, String>,
    over: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut aliases = base;

    for (name, target) in over {
        let key = path_key(&name);
        aliases.retain(|existing, _| path_key(existing) != key);
        aliases.insert(name, target);
    }

    aliases
}
