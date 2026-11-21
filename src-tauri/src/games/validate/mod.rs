//! Rules that need several fields of a definition at once. A field on its own is already checked by its type when the
//! definition is read (see `definition.rs`). The messages are for developers, so they are in English.

mod deploy;
mod loader;
mod problems;

use std::collections::BTreeSet;

use problems::{Problems, is_provider_key, is_variant};

use crate::domain::pathkey;
use crate::games::definition::{GameDef, SUPPORTED_SCHEMA};
use gobbo_overlay_core::MountSpec;

pub(in crate::games) use loader::duplicate_problems;

impl GameDef {
    /// The problems with this definition. An empty list means it is valid.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Problems::default();
        self.check_identity(&mut problems);
        self.check_loader(&mut problems);
        self.check_detect(&mut problems);
        self.check_user_data(&mut problems);
        self.check_deploy(&mut problems);
        self.check_aliases(&mut problems);
        self.check_load_order(&mut problems);
        self.check_steps(&mut problems);
        self.check_tools(&mut problems);
        self.check_logs(&mut problems);
        problems.0
    }

    fn check_identity(&self, problems: &mut Problems) {
        if self.schema == 0 || self.schema > SUPPORTED_SCHEMA {
            problems.push(format!("unknown schema version: {}", self.schema));
        }

        if let Some(min) = &self.min_gobbo {
            problems.min_gobbo("game", min);
        }

        if !crate::games::is_valid_id(&self.id) {
            problems.push(format!(
                "game id must contain only lowercase letters, digits or '-': {:?}",
                self.id
            ));
        }

        if self.name.trim().is_empty() {
            problems.push("game name is empty");
        }

        for (provider, game) in &self.providers {
            if !crate::games::is_valid_id(provider) {
                problems.push(format!(
                    "provider id must contain only lowercase letters, digits or '-': {provider:?}"
                ));
            }

            match &game.key {
                Some(key) if is_provider_key(key) => {}
                _ => problems.push(format!("provider {provider} needs a key for this game")),
            }
        }
    }

    /// A loader reference is a file name and a variant, never a path. Whether the preset and variant exist is checked
    /// when the definition loads (see `loaders::apply`).
    fn check_loader(&self, problems: &mut Problems) {
        let Some(loader) = &self.loader else {
            return;
        };

        if !crate::games::is_valid_id(loader.id()) {
            problems.push(format!(
                "loader id must contain only lowercase letters, digits or '-': {:?}",
                loader.id()
            ));
        }

        if let Some(variant) = loader.variant()
            && !is_variant(variant)
        {
            problems.push(format!(
                "loader variant must contain only lowercase letters, digits, '-', '_' or '.': {variant:?}"
            ));
        }
    }

    fn check_detect(&self, problems: &mut Problems) {
        problems.relative("detect marker", &self.detect.markers);
        problems.relative("detect linux marker", &self.detect.linux_markers);
        problems.relative("required file", &self.requires);
        problems.requirements("game", &self.requirements);
        problems.relative("executable", self.executables.iter().map(|exe| &exe.path));
        problems.relative(
            "Linux executable",
            self.executables
                .iter()
                .filter_map(|exe| exe.path_linux.as_ref()),
        );

        if self.executables.is_empty() {
            problems.push("a game needs at least one executable".to_string());
        }

        for path in &self.profile_data {
            problems.template("profile_data", path);

            if !path.trim().starts_with('{') {
                problems.push(format!(
                    "profile_data must start with a path variable: {path:?}"
                ));
            }
        }
    }

    /// User-data sets: a unique slug, a safe template, and no two sets that overlap. Both platform templates are
    /// checked; the overlap check compares the template each platform uses. The service repeats it on the resolved
    /// paths for this installation.
    fn check_user_data(&self, problems: &mut Problems) {
        if self.user_data.len() > 7 {
            problems.push("a game may declare at most 7 user_data sets".to_string());
        }

        let mut ids = BTreeSet::new();

        for set in &self.user_data {
            if !crate::games::is_valid_id(&set.id) {
                problems.push(format!(
                    "user_data id must contain only lowercase letters, digits or '-': {:?}",
                    set.id
                ));
            }

            if set.id == MountSpec::GAME_ID {
                problems.push(format!(
                    "user_data id {:?} is reserved for the game mount",
                    MountSpec::GAME_ID
                ));
            }

            if !ids.insert(set.id.as_str()) {
                problems.push(format!("user_data id is repeated: {:?}", set.id));
            }

            check_user_data_template(problems, "user_data path", &set.path);

            if let Some(linux) = set
                .path_linux
                .as_deref()
                .filter(|path| !path.trim().is_empty())
            {
                check_user_data_template(problems, "user_data path_linux", linux);
            }
        }
        // A definition is valid on both systems, so a pair that overlaps in either platform's effective templates is
        // reported once.
        let mut pairs = overlapping_user_data(
            self.user_data
                .iter()
                .map(|set| (set.id.as_str(), set.path.as_str())),
        );
        pairs.extend(overlapping_user_data(
            self.user_data
                .iter()
                .map(|set| (set.id.as_str(), set.linux_path())),
        ));

        for (first, second) in pairs {
            problems.push(format!("user_data sets {first:?} and {second:?} overlap"));
        }
    }

    fn check_steps(&self, problems: &mut Problems) {
        let mut ids = BTreeSet::new();

        for step in &self.steps {
            if step.id.is_empty() || !ids.insert(&step.id) {
                problems.push(format!("step id is empty or repeated: {:?}", step.id));
            }

            let what = format!("step {}", step.id);
            problems.relative(&what, [&step.program]);
            problems.relative(
                &what,
                step.when_dir
                    .iter()
                    .chain(&step.clean)
                    .chain(&step.outputs)
                    .chain(&step.log),
            );
        }
    }

    fn check_logs(&self, problems: &mut Problems) {
        if self.logs.len() > 20 {
            problems.push("a game may declare at most 20 log sources".to_string());
        }

        for (index, source) in self.logs.iter().enumerate() {
            let what = format!("log source {}", index + 1);
            problems.relative(&what, [&source.path]);

            if source.pattern.is_empty()
                || source.pattern.contains(['/', '\\'])
                || source.pattern.contains("..")
            {
                problems.push(format!(
                    "{what} pattern must be a file name without folders: {:?}",
                    source.pattern
                ));
            }

            if !(1..=20).contains(&source.limit) {
                problems.push(format!("{what} limit must be between 1 and 20"));
            }
        }
    }
}

/// Whether one normalized path template is the other or lies below it.
fn keys_overlap(first: &str, second: &str) -> bool {
    first == second
        || first
            .strip_prefix(second)
            .is_some_and(|rest| rest.starts_with('/'))
        || second
            .strip_prefix(first)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// A user-data template: not empty, no `..`, starting with a path variable, and not the game folder itself.
fn check_user_data_template(problems: &mut Problems, what: &str, template: &str) {
    problems.template(what, template);
    let template = template.trim();

    if !template.starts_with('{') {
        problems.push(format!(
            "{what} must start with a path variable: {template:?}"
        ));
    }

    if template.starts_with("{game}") {
        problems.push(format!(
            "{what} is inside the game folder; the game mount already covers it: {template:?}"
        ));
    }
}

/// The user-data pairs whose templates are the same or nested, for one platform's effective templates. A pair is kept
/// once whatever the order of the sets.
fn overlapping_user_data<'a>(
    templates: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> BTreeSet<(&'a str, &'a str)> {
    let mut seen: Vec<(&str, String)> = Vec::new();
    let mut pairs = BTreeSet::new();

    for (id, template) in templates {
        let key = pathkey::path_key(template.trim());

        for (other_id, other) in &seen {
            if keys_overlap(&key, other) {
                let pair = if id < *other_id {
                    (id, *other_id)
                } else {
                    (*other_id, id)
                };
                pairs.insert(pair);
            }
        }

        seen.push((id, key));
    }

    pairs
}
