//! The deploy, alias, order and tool rules of a game definition.

use std::collections::BTreeSet;

use super::problems::{Problems, is_sha512};
use crate::domain::deploy::Deploy;
use crate::domain::pathkey;
use crate::games::definition::GameDef;
use crate::games::is_valid_id;
use crate::games::layout::is_game_relative;

impl GameDef {
    pub(super) fn check_deploy(&self, problems: &mut Problems) {
        let deploy = &self.deploy;

        for root in [&deploy.root, &deploy.root_linux].into_iter().flatten() {
            problems.template("deploy root", root);

            if !root.trim().starts_with('{') {
                problems.push(format!(
                    "deploy root must start with a path variable: {root:?}"
                ));
            }
        }

        if deploy.slots.is_empty() && deploy.mirror.is_empty() && !deploy.full_root {
            problems.push("the game needs at least one slot or mirror");
        }

        if deploy.full_root && !deploy.slots.is_empty() {
            problems
                .push("a full-root game cannot use slots; every file already keeps its game path");
        }

        let mut dirs = BTreeSet::new();
        let places = deploy.slots.iter().map(|slot| &slot.dir);

        for dir in places.chain(deploy.mirror.iter().map(|mirror| &mirror.dir)) {
            if !dirs.insert(dir.to_lowercase()) {
                problems.push(format!("folder {dir:?} is a slot or mirror twice"));
            }
        }

        problems.relative("slot", deploy.slots.iter().map(|slot| &slot.dir));
        problems.relative("mirror", deploy.mirror.iter().map(|mirror| &mirror.dir));
        let places: Vec<&str> = deploy_dirs(deploy).collect();

        for (index, left) in places.iter().enumerate() {
            let left_key = pathkey::path_key(left);

            for right in &places[index + 1..] {
                let right_key = pathkey::path_key(right);

                if left_key == right_key
                    || left_key
                        .strip_prefix(right_key.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
                    || right_key
                        .strip_prefix(left_key.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
                {
                    problems.push(format!(
                        "folders {left:?} and {right:?} overlap; a file would have two places"
                    ));
                }
            }
        }

        problems.relative("protected folder", &deploy.protected);

        for slot in &deploy.slots {
            let what = format!("slot {}", slot.dir);

            if slot.flat && slot.marker.is_some() {
                problems.push(format!(
                    "{what}: a flat slot keeps no mod folders, so it cannot use a marker"
                ));
            }

            problems.relative(&what, &slot.marker);
            problems.extensions(
                &what,
                slot.extensions
                    .iter()
                    .chain(&slot.root_extensions)
                    .chain(&slot.root_folder_extensions),
            );
        }

        for mirror in &deploy.mirror {
            problems.extensions(&format!("mirror {}", mirror.dir), &mirror.extensions);
        }

        for (index, rule) in deploy.moves.iter().enumerate() {
            let what = format!("move {}", index + 1);
            problems.extensions(&what, &rule.extensions);

            if rule.extensions.is_empty() {
                problems.push(format!("{what} needs at least one extension"));
            }

            problems.relative(&what, std::iter::once(&rule.to));

            if rule.to.trim().is_empty() || rule.to.ends_with(['/', '\\']) {
                problems.push(format!(
                    "{what} target must be a folder without a trailing slash"
                ));
            }
        }
        // A root file keeps its bare name in the folder that holds the slots and mirrors. No game folder may have that name.
        let folders: BTreeSet<String> = deploy_dirs(deploy)
            .chain(deploy.protected.iter().map(String::as_str))
            .filter_map(|dir| dir.split('/').next())
            .map(pathkey::path_key)
            .collect();
        let mut names = BTreeSet::new();

        for name in &deploy.root_files {
            let key = pathkey::path_key(name);

            if !is_game_relative(name) || name.contains(['/', '\\']) {
                problems.push(format!(
                    "root file {name:?} must be a file name, not a path"
                ));
            } else if folders.contains(&key) {
                problems.push(format!("root file {name:?} has the name of a game folder"));
            } else if !names.insert(key) {
                problems.push(format!("root file {name:?} is listed twice"));
            }
        }

        for shared in &deploy.shared {
            if !shared.dir.is_empty() {
                problems.relative("shared", [&shared.dir]);
            }

            problems.extensions("shared", &shared.extensions);
        }
    }

    /// Archive folder aliases: one folder name each, unique without regard to case, never a game folder itself, and
    /// pointing at a folder inside a slot or mirror.
    pub(super) fn check_aliases(&self, problems: &mut Problems) {
        let deploy = &self.deploy;
        let places: Vec<String> = deploy_dirs(deploy).map(pathkey::path_key).collect();
        let game_folders: BTreeSet<&str> = places
            .iter()
            .filter_map(|place| place.split('/').next())
            .collect();
        let mut keys = BTreeSet::new();

        for (name, target) in &deploy.aliases {
            if name.trim().is_empty() || name.contains(['/', '\\']) || !is_game_relative(name) {
                problems.push(format!("alias {name:?} must be one folder name"));
                continue;
            }

            let key = pathkey::path_key(name);

            if !keys.insert(key.clone()) {
                problems.push(format!("alias {name:?} is repeated (case is ignored)"));
            }

            if game_folders.contains(key.as_str()) {
                problems.push(format!(
                    "alias {name:?} is already a game folder; an archive can use it as it is"
                ));
            }

            if target.contains('\\') {
                problems.push(format!(
                    "alias {name:?} must use '/' in its target: {target:?}"
                ));
            }

            problems.relative("alias target", std::iter::once(target));
            let target_key = pathkey::path_key(target);
            let inside = deploy.full_root
                || places.iter().any(|place| {
                    target_key == *place
                        || target_key
                            .strip_prefix(place.as_str())
                            .is_some_and(|rest| rest.starts_with('/'))
                });
            if !inside {
                problems.push(format!(
                    "alias {name:?} points to {target:?}, which is not inside a slot or mirror"
                ));
            }
        }
    }

    pub(super) fn check_load_order(&self, problems: &mut Problems) {
        let Some(order) = &self.load_order else {
            return;
        };
        problems.template("load_order.file", &order.file);

        if order.file.ends_with(['/', '\\']) {
            problems.push(format!(
                "load_order.file must name a file: {:?}",
                order.file
            ));
        }

        if order.priority_min >= order.priority_max {
            problems.push("priority_min must be less than priority_max");
        }
    }

    pub(super) fn check_tools(&self, problems: &mut Problems) {
        for tool in &self.tools {
            if tool.detect.is_empty() {
                problems.push(format!("tool {} needs at least one detect path", tool.id));
            }

            problems.relative("tool detect path", &tool.detect);

            if let Some(source) = &tool.source {
                if !is_valid_id(source.provider.as_str()) {
                    problems.push(format!(
                        "tool {} names an unknown provider: {:?}",
                        tool.id, source.provider
                    ));
                } else if !self.providers.contains_key(source.provider.as_str()) {
                    problems.push(format!(
                        "tool {} names provider {} which the game does not list",
                        tool.id, source.provider
                    ));
                }

                if source.mod_id.trim().is_empty() {
                    problems.push(format!("tool {} has no source mod id", tool.id));
                }
            }

            for prerequisite in &tool.prerequisites {
                let name = &prerequisite.name;

                if !prerequisite.url.starts_with("https://") {
                    problems.push(format!("prerequisite {name} must be an https link"));
                }

                if !is_sha512(&prerequisite.sha512) {
                    problems.push(format!(
                        "prerequisite {name} needs a SHA-512 as 128 hex digits"
                    ));
                }

                problems.relative("prerequisite check", std::iter::once(&prerequisite.check));

                if prerequisite.check_starts_with.trim().is_empty() {
                    problems.push(format!("prerequisite {name} needs check_starts_with"));
                }
            }
        }
    }
}

/// The slot and mirror folders of a definition, in definition order.
fn deploy_dirs(deploy: &Deploy) -> impl Iterator<Item = &str> {
    deploy
        .slots
        .iter()
        .map(|slot| slot.dir.as_str())
        .chain(deploy.mirror.iter().map(|mirror| mirror.dir.as_str()))
}
