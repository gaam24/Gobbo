//! Loader presets from `loaders/<id>.toml`. A preset carries the `requires` and `[deploy]` rules shared by every game
//! that uses the same loader. A game names one with `loader = "id"` or `loader = { id = "id", variant = "6" }`; the
//! preset's rules are merged under the game's own, so a game can add to or override a preset entry. The code never
//! branches on a loader id: a new loader is a new file.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::definition::{GameDef, Install, Requirement, ResolvedLoader};
use crate::domain::deploy::Deploy;
use crate::domain::pathkey::path_key;

/// One loader preset as its file holds it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LoaderDef {
    pub(super) schema: u32,
    pub(super) id: String,
    pub(super) name: String,
    /// The game-relative folder an unpacked community profile keeps this loader's config files in, when it has one.
    #[serde(default)]
    pub(super) config_mirror: Option<String>,
    /// The oldest Gobbo release that serves this preset correctly, like `min_gobbo` in `game.toml`.
    #[serde(default)]
    pub(super) min_gobbo: Option<String>,
    #[serde(default)]
    pub(super) default_variant: Option<String>,
    #[serde(default)]
    pub(super) requires: Vec<String>,
    #[serde(default)]
    pub(super) requirements: Vec<Requirement>,
    #[serde(default)]
    pub(super) install: Option<Install>,
    #[serde(default)]
    pub(super) deploy: Deploy,
    #[serde(default)]
    pub(super) variants: BTreeMap<String, LoaderVariant>,
}

impl LoaderDef {
    /// Parses one loader file without checking it.
    pub(super) fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// The effective rules for `variant`, or the default variant when it is `None`.
    pub(super) fn rules(&self, variant: Option<&str>) -> Result<LoaderRules, String> {
        let selected = match variant {
            Some(name) => Some(name),
            None if self.variants.is_empty() => None,
            None => Some(self.default_variant.as_deref().ok_or_else(|| {
                format!("loader {} has variants but no default_variant", self.id)
            })?),
        };
        let variant = match selected {
            Some(name) => Some(self.variants.get(name).ok_or_else(|| {
                let available: Vec<&str> = self.variants.keys().map(String::as_str).collect();
                format!(
                    "loader {} has no variant {name:?} (available: {available:?})",
                    self.id
                )
            })?),
            None => None,
        };

        let mut rules = LoaderRules {
            requires: self.requires.clone(),
            requirements: self.requirements.clone(),
            install: self.install.clone(),
            deploy: self.deploy.clone(),
            config_mirror: self.config_mirror.clone(),
        };

        if let Some(variant) = variant {
            rules.requires = merge_lists(rules.requires, variant.requires.clone());
            rules.requirements =
                merge_requirements(rules.requirements, variant.requirements.clone());
            rules.install = merge_installs(rules.install, variant.install.clone());
            rules.deploy = merge_deploy(rules.deploy, variant.deploy.clone());

            if variant.config_mirror.is_some() {
                rules.config_mirror.clone_from(&variant.config_mirror);
            }
        }

        rules.requires = with_requirement_paths(rules.requires, &rules.requirements);
        Ok(rules)
    }
}

/// The rules one variant adds to the preset's base.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LoaderVariant {
    /// The game-relative folder an unpacked community profile keeps this variant's config files in. A value here
    /// replaces the preset's; without one the preset's value stays.
    #[serde(default)]
    pub(super) config_mirror: Option<String>,
    #[serde(default)]
    pub(super) requires: Vec<String>,
    #[serde(default)]
    pub(super) requirements: Vec<Requirement>,
    #[serde(default)]
    pub(super) install: Option<Install>,
    #[serde(default)]
    pub(super) deploy: Deploy,
}

/// The effective rules of a preset: its base, with the selected variant merged over it.
#[derive(Debug, Clone, Default)]
pub(super) struct LoaderRules {
    pub(super) requires: Vec<String>,
    pub(super) requirements: Vec<Requirement>,
    pub(super) install: Option<Install>,
    pub(super) deploy: Deploy,
    pub(super) config_mirror: Option<String>,
}

impl LoaderRules {
    /// Puts these loader rules under the game's own. The game wins a slot, mirror or alias with the same key.
    pub(super) fn apply_to(self, game: &mut GameDef) {
        let game_requires = std::mem::take(&mut game.requires);
        game.requires = merge_lists(self.requires, game_requires);
        let game_requirements = std::mem::take(&mut game.requirements);
        game.requirements = merge_requirements(self.requirements, game_requirements);
        let game_install = std::mem::take(&mut game.install);
        game.install = merge_installs(self.install, game_install);
        let game_deploy = std::mem::take(&mut game.deploy);
        game.deploy = merge_deploy(self.deploy, game_deploy);
    }
}

#[cfg(test)]
use std::path::Path;

#[cfg(test)]
pub(super) fn load(dir: &Path, id: &str) -> Result<LoaderDef, String> {
    load_any(&[dir.to_path_buf()], id)
}

/// Reads and checks `loaders/<id>.toml` from the first folder that has it.
pub(super) fn load_any(dirs: &[std::path::PathBuf], id: &str) -> Result<LoaderDef, String> {
    if !super::is_valid_id(id) {
        return Err(format!("loader id is not a slug: {id:?}"));
    }

    let path = dirs
        .iter()
        .map(|dir| dir.join(format!("{id}.toml")))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            let first = dirs.first().map_or_else(String::new, |dir| {
                dir.join(format!("{id}.toml")).display().to_string()
            });
            format!("{first}: loader {id:?} was not found")
        })?;
    let text =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let loader = LoaderDef::parse(&text).map_err(|error| format!("{}: {error}", path.display()))?;

    if loader.id != id {
        return Err(format!(
            "{}: id {:?} does not match the file name",
            path.display(),
            loader.id
        ));
    }

    let problems = loader.validate();

    if problems.is_empty() {
        Ok(loader)
    } else {
        Err(format!("{}: {}", path.display(), problems.join("; ")))
    }
}

/// Merges the loader named by `game.loader` under the game's own rules, then finishes the game's `requires`: every
/// requirement path is required, with or without a loader. This is the one place a game's rules are finalized, for
/// shipped and user definitions alike. The folders are searched in order, so a user preset can replace a bundled one
/// with the same id.
pub(super) fn apply(game: &mut GameDef, dirs: &[std::path::PathBuf]) -> Result<(), String> {
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
pub(super) fn with_requirement_paths(
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
pub(super) fn merge_lists(mut base: Vec<String>, over: Vec<String>) -> Vec<String> {
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
fn merge_installs(base: Option<Install>, over: Option<Install>) -> Option<Install> {
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
fn merge_requirements(mut base: Vec<Requirement>, over: Vec<Requirement>) -> Vec<Requirement> {
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
fn merge_deploy(base: Deploy, over: Deploy) -> Deploy {
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
