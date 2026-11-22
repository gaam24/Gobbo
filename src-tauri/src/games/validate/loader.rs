//! The loader preset rules, and the duplicate rules that a merge would otherwise hide.

use std::collections::BTreeSet;

use super::install::check_install;
use super::problems::{Problems, is_variant};
use crate::domain::deploy::Deploy;
use crate::domain::pathkey;
use crate::games::is_valid_id;
use crate::games::loaders::LoaderDef;

impl LoaderDef {
    /// The problems with this loader. An empty list means it is valid. The deploy rules themselves are checked on the
    /// merged game, because an alias may point at a place the game contributes.
    pub(in crate::games) fn validate(&self) -> Vec<String> {
        let mut problems = Problems::default();

        if self.schema != 1 {
            problems.push(format!("unknown schema version: {}", self.schema));
        }

        if let Some(min) = &self.min_gobbo {
            problems.min_gobbo("loader", min);
        }

        if !is_valid_id(&self.id) {
            problems.push(format!(
                "loader id must contain only lowercase letters, digits or '-': {:?}",
                self.id
            ));
        }

        if self.name.trim().is_empty() {
            problems.push("loader name is empty".to_string());
        }

        if self.variants.is_empty() {
            if self.default_variant.is_some() {
                problems.push("default_variant is set but the loader has no variants".to_string());
            }
        } else if let Some(name) = self.default_variant.as_deref() {
            if !self.variants.contains_key(name) {
                problems.push(format!(
                    "default_variant {name:?} is not one of the variants"
                ));
            }
        } else {
            problems.push("a loader with variants needs default_variant".to_string());
        }

        for name in self.variants.keys() {
            if !is_variant(name) {
                problems.push(format!(
                    "variant name must contain only lowercase letters, digits, '-', '_' or '.': {name:?}"
                ));
            }
        }

        check_loader_root(&self.deploy, "loader", &mut problems);
        problems.0.extend(duplicate_problems(&self.deploy));
        problems.relative("required file", &self.requires);
        problems.requirements("loader", &self.requirements);
        if let Some(install) = &self.install {
            check_install("loader install", install, &mut problems);
        }

        for (name, variant) in &self.variants {
            let what = format!("variant {name}");
            check_loader_root(&variant.deploy, &what, &mut problems);
            problems.0.extend(duplicate_problems(&variant.deploy));
            problems.relative(&format!("{what} required file"), &variant.requires);
            problems.requirements(&what, &variant.requirements);
            if let Some(install) = &variant.install {
                check_install(&format!("{what} install"), install, &mut problems);
            }
        }

        problems.0
    }
}

/// The duplicate rules that a merge would hide: two places with one folder name, two aliases with one name, or one
/// root file listed twice. Each layer is checked before the layers are merged.
pub(in crate::games) fn duplicate_problems(deploy: &Deploy) -> Vec<String> {
    let mut problems = Problems::default();
    let mut dirs = BTreeSet::new();
    let places = deploy.slots.iter().map(|slot| &slot.dir);

    for dir in places.chain(deploy.mirror.iter().map(|mirror| &mirror.dir)) {
        if !dirs.insert(dir.to_lowercase()) {
            problems.push(format!("folder {dir:?} is a slot or mirror twice"));
        }
    }

    let mut aliases = BTreeSet::new();

    for name in deploy.aliases.keys() {
        if !aliases.insert(pathkey::path_key(name)) {
            problems.push(format!("alias {name:?} is repeated (case is ignored)"));
        }
    }

    let mut names = BTreeSet::new();

    for name in &deploy.root_files {
        if !names.insert(pathkey::path_key(name)) {
            problems.push(format!("root file {name:?} is listed twice"));
        }
    }

    problems.0
}

/// A loader never chooses the deployment root; the game does, because only it knows where its mods live.
fn check_loader_root(deploy: &Deploy, what: &str, problems: &mut Problems) {
    if deploy.root.is_some() || deploy.root_linux.is_some() {
        problems.push(format!(
            "{what} cannot set deploy.root or deploy.root_linux; the game owns the deployment root"
        ));
    }
}
