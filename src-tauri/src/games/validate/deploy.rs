//! The deploy, alias, order and tool rules of a game definition.

use std::collections::BTreeSet;

use super::problems::{Duplicates, Problems, Section, is_sha512};
use crate::domain::deploy::Deploy;
use crate::domain::pathkey;
use crate::games::definition::GameDef;
use crate::games::is_valid_id;
use crate::games::layout::is_game_relative;

impl GameDef {
    pub(super) fn check_deploy(&self, problems: &mut Problems) {
        let deploy = &self.deploy;

        for root in [&deploy.root, &deploy.root_linux].into_iter().flatten() {
            problems.template(Section::Deploy, "deploy root", root);

            if !root.trim().starts_with('{') {
                problems.push_in(
                    Section::Deploy,
                    format!("deploy root must start with a path variable: {root:?}"),
                );
            }
        }

        if deploy.slots.is_empty() && deploy.mirror.is_empty() && !deploy.full_root {
            problems.push_in(
                Section::Deploy,
                "the game needs at least one slot or mirror",
            );
        }

        if deploy.full_root && !deploy.slots.is_empty() {
            problems.push_in(
                Section::Deploy,
                "a full-root game cannot use slots; every file already keeps its game path",
            );
        }

        let mut duplicates = Duplicates::default();
        let places = deploy.slots.iter().map(|slot| &slot.dir);

        for dir in places.chain(deploy.mirror.iter().map(|mirror| &mirror.dir)) {
            duplicates.folder(dir, Section::Deploy, problems);
        }

        problems.relative(
            Section::Deploy,
            "slot",
            deploy.slots.iter().map(|slot| &slot.dir),
        );
        problems.relative(
            Section::Deploy,
            "mirror",
            deploy.mirror.iter().map(|mirror| &mirror.dir),
        );
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
                    problems.push_in(
                        Section::Source,
                        format!(
                            "folders {left:?} and {right:?} overlap; a file would have two places"
                        ),
                    );
                }
            }
        }

        problems.relative(Section::Deploy, "protected folder", &deploy.protected);

        for slot in &deploy.slots {
            let what = format!("slot {}", slot.dir);

            if slot.flat && (slot.marker.is_some() || !slot.markers.is_empty()) {
                problems.push_in(
                    Section::Deploy,
                    format!("{what}: a flat slot keeps no mod folders, so it cannot use a marker"),
                );
            }

            if slot.flat && slot.enable_file.is_some() {
                problems.push_in(
                    Section::Deploy,
                    format!(
                        "{what}: a flat slot keeps no mod folders, so it cannot write an enable file"
                    ),
                );
            }

            if let Some(file) = &slot.enable_file {
                let valid = !file.is_empty()
                    && file.trim() == file
                    && !file.contains(['/', '\\'])
                    && crate::domain::naming::safe_file_name(file) == *file;

                if !valid {
                    problems.push_in(
                        Section::Deploy,
                        format!("{what}: enable_file must be one valid file name: {file:?}"),
                    );
                }
            }

            problems.relative(Section::Deploy, &what, &slot.marker);
            problems.relative(Section::Deploy, &what, &slot.markers);
            problems.extensions(
                Section::Deploy,
                &what,
                slot.extensions
                    .iter()
                    .chain(&slot.root_extensions)
                    .chain(&slot.root_folder_extensions),
            );
        }

        for mirror in &deploy.mirror {
            problems.extensions(
                Section::Deploy,
                &format!("mirror {}", mirror.dir),
                &mirror.extensions,
            );
        }

        for (index, rule) in deploy.moves.iter().enumerate() {
            let what = format!("move {}", index + 1);
            problems.extensions(Section::Source, &what, &rule.extensions);

            if rule.extensions.is_empty() {
                problems.push_in(
                    Section::Source,
                    format!("{what} needs at least one extension"),
                );
            }

            problems.relative(Section::Source, &what, std::iter::once(&rule.to));

            if rule.to.trim().is_empty() || rule.to.ends_with(['/', '\\']) {
                problems.push_in(
                    Section::Source,
                    format!("{what} target must be a folder without a trailing slash"),
                );
            }
        }
        // A root file keeps its bare name in the folder that holds the slots and mirrors. No game folder may have that name.
        let folders: BTreeSet<String> = deploy_dirs(deploy)
            .chain(deploy.protected.iter().map(String::as_str))
            .filter_map(|dir| dir.split('/').next())
            .map(pathkey::path_key)
            .collect();

        for name in &deploy.root_files {
            let key = pathkey::path_key(name);

            if !is_game_relative(name) || name.contains(['/', '\\']) {
                problems.push_in(
                    Section::Deploy,
                    format!("root file {name:?} must be a file name, not a path"),
                );
            } else if folders.contains(&key) {
                problems.push_in(
                    Section::Deploy,
                    format!("root file {name:?} has the name of a game folder"),
                );
            } else {
                duplicates.root_file(name, Section::Deploy, problems);
            }
        }

        for shared in &deploy.shared {
            if !shared.dir.is_empty() {
                problems.relative(Section::Deploy, "shared", [&shared.dir]);
            }

            problems.extensions(Section::Deploy, "shared", &shared.extensions);
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
        let mut duplicates = Duplicates::default();

        for (name, target) in &deploy.aliases {
            if name.trim().is_empty() || name.contains(['/', '\\']) || !is_game_relative(name) {
                problems.push_in(
                    Section::Deploy,
                    format!("alias {name:?} must be one folder name"),
                );
                continue;
            }

            duplicates.alias(name, Section::Deploy, problems);
            let key = pathkey::path_key(name);

            if game_folders.contains(key.as_str()) {
                problems.push_in(
                    Section::Deploy,
                    format!(
                        "alias {name:?} is already a game folder; an archive can use it as it is"
                    ),
                );
            }

            if target.contains('\\') {
                problems.push_in(
                    Section::Deploy,
                    format!("alias {name:?} must use '/' in its target: {target:?}"),
                );
            }

            problems.relative(Section::Deploy, "alias target", std::iter::once(target));
            let target_key = pathkey::path_key(target);
            let inside = deploy.full_root
                || places.iter().any(|place| {
                    target_key == *place
                        || target_key
                            .strip_prefix(place.as_str())
                            .is_some_and(|rest| rest.starts_with('/'))
                });
            if !inside {
                problems.push_in(
                    Section::Deploy,
                    format!(
                        "alias {name:?} points to {target:?}, which is not inside a slot or mirror"
                    ),
                );
            }
        }
    }

    pub(super) fn check_load_order(&self, problems: &mut Problems) {
        let Some(order) = &self.load_order else {
            return;
        };

        if order.uses_settings_folder() {
            problems.template(Section::LoadOrder, "load_order.file", &order.file);
        } else {
            // A UE4SS list sits in the deployment root and names mod folders, so it must lie in a slot's folder,
            // where those mods are. The file name is not checked: a loader may ship another one.
            problems.relative(
                Section::LoadOrder,
                "load_order.file",
                std::iter::once(&order.file),
            );
            let in_slot = order.file.rsplit_once('/').is_some_and(|(folder, _)| {
                self.deploy
                    .slots
                    .iter()
                    .any(|slot| pathkey::path_key(&slot.dir) == pathkey::path_key(folder))
            });

            if !in_slot {
                problems.push_in(
                    Section::Deploy,
                    format!(
                        "load_order.file must lie in a slot folder of the game: {:?}",
                        order.file
                    ),
                );
            }
        }

        if order.file.ends_with(['/', '\\']) {
            problems.push_in(
                Section::LoadOrder,
                format!("load_order.file must name a file: {:?}", order.file),
            );
        }

        if order.priority_min >= order.priority_max {
            problems.push_in(
                Section::Source,
                "priority_min must be less than priority_max",
            );
        }
    }

    pub(super) fn check_tools(&self, problems: &mut Problems) {
        for tool in &self.tools {
            if tool.detect.is_empty() {
                problems.push_in(
                    Section::Tools,
                    format!("tool {} needs at least one detect path", tool.id),
                );
            }

            problems.relative(Section::Tools, "tool detect path", &tool.detect);

            if let Some(source) = &tool.source {
                if !is_valid_id(source.provider.as_str()) {
                    problems.push_in(
                        Section::Tools,
                        format!(
                            "tool {} names an unknown provider: {:?}",
                            tool.id, source.provider
                        ),
                    );
                } else if !self.providers.contains_key(source.provider.as_str()) {
                    problems.push_in(
                        Section::Tools,
                        format!(
                            "tool {} names provider {} which the game does not list",
                            tool.id, source.provider
                        ),
                    );
                }

                if source.mod_id.trim().is_empty() {
                    problems.push_in(
                        Section::Tools,
                        format!("tool {} has no source mod id", tool.id),
                    );
                }
            }

            for prerequisite in &tool.prerequisites {
                let name = &prerequisite.name;

                if !prerequisite.url.starts_with("https://") {
                    problems.push_in(
                        Section::Tools,
                        format!("prerequisite {name} must be an https link"),
                    );
                }

                if !is_sha512(&prerequisite.sha512) {
                    problems.push_in(
                        Section::Tools,
                        format!("prerequisite {name} needs a SHA-512 as 128 hex digits"),
                    );
                }

                problems.relative(
                    Section::Tools,
                    "prerequisite check",
                    std::iter::once(&prerequisite.check),
                );

                if prerequisite.check_starts_with.trim().is_empty() {
                    problems.push_in(
                        Section::Tools,
                        format!("prerequisite {name} needs check_starts_with"),
                    );
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

#[cfg(test)]
mod tests {
    use super::Section;
    use crate::games::definition::GameDef;
    use crate::test_support::games::SLOT;

    #[test]
    fn load_order_problems_keep_their_legacy_sections() {
        let text = SLOT.replacen(
            "format = \"mods_settings\"\nfile = \"{documents}/Slot Game/mods.settings\"",
            "format = \"ue4ss_mods_txt\"\nfile = \"Other/mods.txt\"\npriority_min = 5\npriority_max = 5",
            1,
        );
        let problems = GameDef::parse(&text).unwrap().validate();
        let section_of = |message: &str| {
            problems
                .iter()
                .find(|problem| problem.message.starts_with(message))
                .map(|problem| problem.section)
                .expect("the load order check reports the problem")
        };

        assert_eq!(
            section_of("load_order.file must lie in a slot folder of the game"),
            Section::Deploy
        );
        assert_eq!(
            section_of("priority_min must be less than priority_max"),
            Section::Source
        );
    }
}
