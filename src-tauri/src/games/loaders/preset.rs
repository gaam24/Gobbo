//! The preset types and the rules that select a variant of one.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::super::definition::{GameDef, LoadOrder, LoaderPackage, Requirement};
use super::merge::{
    merge_deploy, merge_installs, merge_lists, merge_requirements, with_requirement_paths,
};
use crate::domain::deploy::Deploy;

/// One loader preset as its file holds it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::games) struct LoaderDef {
    pub(in crate::games) schema: u32,
    pub(in crate::games) id: String,
    pub(in crate::games) name: String,
    /// The game-relative folder an unpacked community profile keeps this loader's config files in, when it has one.
    #[serde(default)]
    pub(in crate::games) config_mirror: Option<String>,
    /// The oldest Gobbo release that serves this preset correctly, like `min_gobbo` in `game.toml`.
    #[serde(default)]
    pub(in crate::games) min_gobbo: Option<String>,
    #[serde(default)]
    pub(in crate::games) default_variant: Option<String>,
    #[serde(default)]
    pub(in crate::games) requires: Vec<String>,
    #[serde(default)]
    pub(in crate::games) requirements: Vec<Requirement>,
    #[serde(default)]
    pub(in crate::games) install: Option<LoaderPackage>,
    #[serde(default)]
    pub(in crate::games) deploy: Deploy,
    #[serde(default)]
    pub(in crate::games) variants: BTreeMap<String, LoaderVariant>,
    /// How the editor recognises a game folder as this preset's engine. A suggestion only: a game takes nothing from it.
    #[serde(default)]
    pub(in crate::games) engine: Option<EngineDef>,
}

impl LoaderDef {
    /// Parses one loader file without checking it.
    pub(in crate::games) fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// The effective rules for `variant`, or the default variant when it is `None`.
    pub(in crate::games) fn rules(&self, variant: Option<&str>) -> Result<LoaderRules, String> {
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
            load_order: None,
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

            rules.load_order.clone_from(&variant.load_order);
        }

        rules.requires = with_requirement_paths(rules.requires, &rules.requirements);
        Ok(rules)
    }
}

/// The rules one variant adds to the preset's base.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::games) struct LoaderVariant {
    /// The game-relative folder an unpacked community profile keeps this variant's config files in. A value here
    /// replaces the preset's; without one the preset's value stays.
    #[serde(default)]
    pub(in crate::games) config_mirror: Option<String>,
    #[serde(default)]
    pub(in crate::games) requires: Vec<String>,
    #[serde(default)]
    pub(in crate::games) requirements: Vec<Requirement>,
    #[serde(default)]
    pub(in crate::games) install: Option<LoaderPackage>,
    #[serde(default)]
    pub(in crate::games) deploy: Deploy,
    /// The order file this layout's loader reads. A variant names it because each release keeps it in its own folder.
    /// The game's own `load_order` wins when it has one.
    #[serde(default)]
    pub(in crate::games) load_order: Option<LoadOrder>,
}

/// The `[engine]` table of a preset: how the editor recognises a game folder as this engine. Every marker is a folder
/// pattern (see `domain::pattern`), or several joined with `|` (see `marker_alternatives`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::games) struct EngineDef {
    /// The engine's display name, for example `Unity`.
    pub(in crate::games) name: String,
    /// The markers that must all exist in the game folder; each one may list alternatives.
    pub(in crate::games) markers: Vec<String>,
    /// The variant suggested when no entry of `variants` matches. Without it the preset's default variant is suggested.
    #[serde(default)]
    pub(in crate::games) variant: Option<String>,
    /// A `*/` pattern for the project folder, for example `*/Content/Paks`. The folder its first segment matches becomes
    /// the game's deployment root, `{game}/<folder>`.
    #[serde(default)]
    pub(in crate::games) root: Option<String>,
    /// The variants a folder can match by their own markers. The first entry whose markers all exist is used.
    #[serde(default)]
    pub(in crate::games) variants: Vec<EngineVariant>,
}

/// One variant an engine can match, with the markers that show it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::games) struct EngineVariant {
    /// The preset variant this entry selects.
    pub(in crate::games) variant: String,
    /// A short name the editor shows next to the engine, for example `IL2CPP`.
    #[serde(default)]
    pub(in crate::games) label: Option<String>,
    /// The markers that must all exist in the game folder for this variant to match; each one may list alternatives.
    pub(in crate::games) markers: Vec<String>,
}

/// The alternatives of one engine marker. A build that names a file differently (`UnityPlayer.dll` on Windows,
/// `UnityPlayer.so` on Linux) lists both with `|` between them, and the marker matches when one alternative does. Each
/// alternative is trimmed, so `a.dll | a.so` reads the same.
pub(in crate::games) fn marker_alternatives(marker: &str) -> impl Iterator<Item = &str> {
    marker.split('|').map(str::trim)
}

/// The effective rules of a preset: its base, with the selected variant merged over it.
#[derive(Debug, Clone, Default)]
pub(in crate::games) struct LoaderRules {
    pub(in crate::games) requires: Vec<String>,
    pub(in crate::games) requirements: Vec<Requirement>,
    pub(in crate::games) install: Option<LoaderPackage>,
    pub(in crate::games) deploy: Deploy,
    pub(in crate::games) config_mirror: Option<String>,
    pub(in crate::games) load_order: Option<LoadOrder>,
}

impl LoaderRules {
    /// Puts these loader rules under the game's own. The game wins a slot, mirror or alias with the same key.
    pub(in crate::games) fn apply_to(self, game: &mut GameDef) {
        let game_requires = std::mem::take(&mut game.requires);
        game.requires = merge_lists(self.requires, game_requires);
        let game_requirements = std::mem::take(&mut game.requirements);
        game.requirements = merge_requirements(self.requirements, game_requirements);
        let game_install = std::mem::take(&mut game.install);
        game.install = merge_installs(self.install, game_install);
        let game_deploy = std::mem::take(&mut game.deploy);
        game.deploy = merge_deploy(self.deploy, game_deploy);

        if game.load_order.is_none() {
            game.load_order = self.load_order;
        }
    }
}
