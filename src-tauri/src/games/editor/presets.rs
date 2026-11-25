//! The loader presets the editor offers: the user's files first, then the official catalog, then the bundled folder.
//! A preset in an earlier folder replaces the same id in a later one. An invalid file stays in the list with its
//! problems: the window names it, and the rest still works.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use super::super::loaders::{
    EngineDef, LoaderDef, LoaderRules, merge_lists, with_requirement_paths,
};
use super::super::{GameOrigin, catalog_loaders_dir, loaders_dir, user_loaders_dir};
use super::{LoaderEngineVariantView, LoaderEngineView, LoaderPresetView, LoaderVariantView};
use crate::domain::deploy::Deploy;

/// The loader presets the editor offers: the user's first, the official catalog second, the bundled folder last. A
/// preset in an earlier folder replaces the same id in a later one. An invalid file is returned with its problems
/// instead of hiding the rest, so the window can name the file the user needs to fix.
pub fn loader_presets() -> Vec<LoaderPresetView> {
    let mut dirs = Vec::with_capacity(3);

    if let Some(user) = user_loaders_dir() {
        dirs.push((user, GameOrigin::User));
    }

    if let Some(catalog) = catalog_loaders_dir() {
        dirs.push((catalog, GameOrigin::Official));
    }

    dirs.push((loaders_dir().to_path_buf(), GameOrigin::Bundled));
    presets_from(&dirs)
}

pub(super) fn presets_from(dirs: &[(PathBuf, GameOrigin)]) -> Vec<LoaderPresetView> {
    let mut seen = BTreeSet::new();
    let mut presets = Vec::new();

    for (dir, origin) in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = file.strip_suffix(".toml") else {
                continue;
            };

            if !seen.insert(id.to_string()) {
                continue;
            }

            let path = entry.path();
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    presets.push(failed_preset(id, *origin, error.to_string()));
                    continue;
                }
            };
            let loader = match LoaderDef::parse(&text) {
                Ok(loader) => loader,
                Err(error) => {
                    presets.push(failed_preset(id, *origin, error.to_string()));
                    continue;
                }
            };
            let mut problems = loader.validate();

            if loader.id != id {
                problems.push(format!("id {:?} does not match the file name", loader.id));
            }

            if !problems.is_empty() {
                presets.push(LoaderPresetView {
                    id: id.to_string(),
                    name: loader.name,
                    origin: *origin,
                    default_variant: loader.default_variant,
                    requires: with_requirement_paths(loader.requires, &loader.requirements),
                    places: places_of(&loader.deploy),
                    root_files: loader.deploy.root_files,
                    variants: Vec::new(),
                    problems,
                    engine: None,
                });
                continue;
            }

            let mut variants = Vec::new();

            for name in loader.variants.keys() {
                match loader.rules(Some(name)) {
                    Ok(rules) => variants.push(variant_view(name, &rules)),
                    Err(detail) => {
                        problems.push(format!("variant {name}: {detail}"));
                    }
                }
            }

            presets.push(LoaderPresetView {
                id: loader.id,
                name: loader.name,
                origin: *origin,
                default_variant: loader.default_variant,
                requires: with_requirement_paths(loader.requires, &loader.requirements),
                places: places_of(&loader.deploy),
                root_files: loader.deploy.root_files,
                variants,
                problems,
                engine: loader.engine.as_ref().map(engine_view),
            });
        }
    }

    presets.sort_by(|left, right| left.id.cmp(&right.id));
    presets
}

/// A preset that could not be read. It stays in the list with its reason, so the window can name the file to fix.
fn failed_preset(id: &str, origin: GameOrigin, problem: String) -> LoaderPresetView {
    LoaderPresetView {
        id: id.to_string(),
        name: id.to_string(),
        origin,
        default_variant: None,
        requires: Vec::new(),
        places: Vec::new(),
        root_files: Vec::new(),
        variants: Vec::new(),
        problems: vec![problem],
        engine: None,
    }
}

/// The engine rules of a preset as the editor reads them. The checks already ran when the preset loaded.
fn engine_view(engine: &EngineDef) -> LoaderEngineView {
    LoaderEngineView {
        name: engine.name.clone(),
        markers: engine.markers.clone(),
        variant: engine.variant.clone(),
        root: engine.root.clone(),
        variants: engine
            .variants
            .iter()
            .map(|variant| LoaderEngineVariantView {
                variant: variant.variant.clone(),
                label: variant.label.clone(),
                markers: variant.markers.clone(),
            })
            .collect(),
    }
}

/// A variant as the editor shows it. The files that prove its loader is there are its required files and the check
/// files of its package, so a variant whose loader comes from a package is still recognised in a game folder.
fn variant_view(name: &str, rules: &LoaderRules) -> LoaderVariantView {
    let checks = rules
        .install
        .as_ref()
        .map(|install| install.check.clone())
        .unwrap_or_default();
    LoaderVariantView {
        name: name.to_string(),
        requires: merge_lists(rules.requires.clone(), checks),
        places: places_of(&rules.deploy),
        root_files: rules.deploy.root_files.clone(),
    }
}

fn places_of(deploy: &Deploy) -> Vec<String> {
    deploy
        .slots
        .iter()
        .map(|slot| slot.dir.clone())
        .chain(deploy.mirror.iter().map(|mirror| mirror.dir.clone()))
        .collect()
}
