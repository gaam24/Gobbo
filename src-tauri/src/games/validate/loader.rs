//! The loader preset rules, and the duplicate rules that a merge would otherwise hide.

use std::collections::{BTreeMap, BTreeSet};

use super::install::check_install;
use super::problems::{Problems, Section, duplicate_problems, is_variant};
use crate::domain::deploy::Deploy;
use crate::domain::pathkey;
use crate::domain::pattern;
use crate::games::is_valid_id;
use crate::games::loaders::{EngineDef, LoaderDef, LoaderVariant, marker_alternatives};

impl LoaderDef {
    /// The problems with this loader. An empty list means it is valid. The deploy rules themselves are checked on the
    /// merged game, because an alias may point at a place the game contributes.
    pub(in crate::games) fn validate(&self) -> Vec<String> {
        let mut problems = Problems::default();

        if self.schema != 1 {
            problems.push_in(
                Section::Loader,
                format!("unknown schema version: {}", self.schema),
            );
        }

        if let Some(min) = &self.min_gobbo {
            problems.min_gobbo(Section::Loader, "loader", min);
        }

        if !is_valid_id(&self.id) {
            problems.push_in(
                Section::Loader,
                format!(
                    "loader id must contain only lowercase letters, digits or '-': {:?}",
                    self.id
                ),
            );
        }

        if self.name.trim().is_empty() {
            problems.push_in(Section::Loader, "loader name is empty".to_string());
        }

        if self.variants.is_empty() {
            if self.default_variant.is_some() {
                problems.push_in(
                    Section::Loader,
                    "default_variant is set but the loader has no variants".to_string(),
                );
            }
        } else if let Some(name) = self.default_variant.as_deref() {
            if !self.variants.contains_key(name) {
                problems.push_in(
                    Section::Loader,
                    format!("default_variant {name:?} is not one of the variants"),
                );
            }
        } else {
            problems.push_in(
                Section::Loader,
                "a loader with variants needs default_variant".to_string(),
            );
        }

        for name in self.variants.keys() {
            if !is_variant(name) {
                problems.push_in(
                    Section::Loader,
                    format!(
                        "variant name must contain only lowercase letters, digits, '-', '_' or '.': {name:?}"
                    ),
                );
            }
        }

        check_loader_root(&self.deploy, "loader", &mut problems);
        problems
            .0
            .extend(duplicate_problems(&self.deploy, Section::Loader));
        problems.relative(Section::Loader, "required file", &self.requires);
        problems.requirements(Section::Loader, "loader", &self.requirements);
        check_config_mirror(
            self.config_mirror.as_ref(),
            "loader",
            &self.deploy,
            None,
            &mut problems,
        );
        if let Some(install) = &self.install {
            check_install(Section::Loader, "loader install", install, &mut problems);
        }

        for (name, variant) in &self.variants {
            let what = format!("variant {name}");
            check_config_mirror(
                variant.config_mirror.as_ref(),
                &what,
                &self.deploy,
                Some(&variant.deploy),
                &mut problems,
            );
            check_loader_root(&variant.deploy, &what, &mut problems);
            problems
                .0
                .extend(duplicate_problems(&variant.deploy, Section::Loader));
            problems.relative(
                Section::Loader,
                &format!("{what} required file"),
                &variant.requires,
            );
            problems.requirements(Section::Loader, &what, &variant.requirements);
            if let Some(install) = &variant.install {
                check_install(
                    Section::Loader,
                    &format!("{what} install"),
                    install,
                    &mut problems,
                );
            }
        }

        if let Some(engine) = &self.engine {
            check_engine(engine, &self.variants, &mut problems);
        }

        problems
            .0
            .into_iter()
            .map(|problem| problem.message)
            .collect()
    }
}

/// `config_mirror` names a real folder of this preset: a relative game path that the base rules declare (or, for a
/// variant, that the base rules or the variant's own rules declare). A slot counts as well as a mirror or alias target:
/// a loader such as UE4SS keeps its config in the folder its mods use.
fn check_config_mirror(
    mirror: Option<&String>,
    what: &str,
    base: &Deploy,
    variant: Option<&Deploy>,
    problems: &mut Problems,
) {
    let Some(folder) = mirror else {
        return;
    };

    problems.relative(
        Section::Loader,
        &format!("{what} config_mirror"),
        std::iter::once(folder),
    );

    let mut declared: Vec<&str> = Vec::new();

    for deploy in std::iter::once(base).chain(variant) {
        declared.extend(deploy.slots.iter().map(|slot| slot.dir.as_str()));
        declared.extend(deploy.mirror.iter().map(|mirror| mirror.dir.as_str()));
        declared.extend(deploy.aliases.values().map(String::as_str));
    }

    let key = pathkey::path_key(folder);

    if !declared.iter().any(|dir| pathkey::path_key(dir) == key) {
        problems.push_in(
            Section::Loader,
            format!(
                "{what} config_mirror {folder:?} is not a slot, mirror or alias target of the preset"
            ),
        );
    }
}

/// A loader never chooses the deployment root; the game does, because only it knows where its mods live.
fn check_loader_root(deploy: &Deploy, what: &str, problems: &mut Problems) {
    if deploy.root.is_some() || deploy.root_linux.is_some() {
        problems.push_in(
            Section::Loader,
            format!(
                "{what} cannot set deploy.root or deploy.root_linux; the game owns the deployment root"
            ),
        );
    }
}

/// The `[engine]` table: a name, markers that are folder patterns, and variants the preset really has. The project
/// `root` starts with `*/`, so its first segment is the folder the game's mods sit in.
fn check_engine(
    engine: &EngineDef,
    variants: &BTreeMap<String, LoaderVariant>,
    problems: &mut Problems,
) {
    if engine.name.trim().is_empty() {
        problems.push_in(Section::Loader, "engine name is empty".to_string());
    }

    check_engine_markers("engine", &engine.markers, problems);

    if let Some(name) = &engine.variant
        && !variants.contains_key(name)
    {
        problems.push_in(
            Section::Loader,
            format!("engine variant {name:?} is not one of the variants"),
        );
    }

    let mut named = BTreeSet::new();

    for candidate in &engine.variants {
        let what = format!("engine variant {:?}", candidate.variant);

        if !variants.contains_key(&candidate.variant) {
            problems.push_in(
                Section::Loader,
                format!("{what} is not one of the variants"),
            );
        }

        if !named.insert(candidate.variant.as_str()) {
            problems.push_in(Section::Loader, format!("{what} is listed twice"));
        }

        if candidate
            .label
            .as_deref()
            .is_some_and(|label| label.trim().is_empty())
        {
            problems.push_in(Section::Loader, format!("{what} label is empty"));
        }

        check_engine_markers(&what, &candidate.markers, problems);
    }

    if let Some(root) = &engine.root
        && (!root.starts_with("*/") || !pattern::is_valid(root) || root.contains('|'))
    {
        problems.push_in(
            Section::Loader,
            format!("engine root must be a folder pattern that starts with */: {root:?}"),
        );
    }
}

/// The markers of one engine or variant: at least one, and each alternative of each a folder pattern inside the game.
/// Alternatives are joined with `|`, so an empty one (`a.dll|` or `a.dll||a.so`) is an error.
fn check_engine_markers(what: &str, markers: &[String], problems: &mut Problems) {
    if markers.is_empty() {
        problems.push_in(Section::Loader, format!("{what} needs at least one marker"));
    }

    for marker in markers {
        if !marker_alternatives(marker).all(pattern::is_valid) {
            problems.push_in(
                Section::Loader,
                format!("{what} marker is not a folder pattern inside the game: {marker:?}"),
            );
        }
    }
}
