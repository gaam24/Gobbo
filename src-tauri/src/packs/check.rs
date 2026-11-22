//! Checking a pack before it is exported or imported.

use super::model::PackArchives;
use super::model::PackIssue;
use super::model::PackOptions;
use super::model::PackProfile;
use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::deploy::target::build_plan_with_mode;
use crate::deploy::target::is_ready;
use crate::domain::library::LibraryView;
use crate::domain::resolver::PlanMode;
use crate::error::AppError;
use crate::error::Message;
use crate::library::relations::relation_warnings;

/// What stands in the way of sharing the chosen profiles, before the pack is written.
pub fn check_pack(
    root: &Data,
    game_id: &str,
    options: &PackOptions,
) -> Result<Vec<PackIssue>, AppError> {
    let Loaded {
        game,
        data,
        settings,
        library,
        profiles,
        ..
    } = load_game(root, game_id)?;
    let view = LibraryView::new(&library);
    let steps = crate::launch::steps::load_mod_steps(&data)?;
    let mut issues = Vec::new();
    let issue = |error: bool, message: Message| PackIssue { error, message };

    if options.info.name.trim().is_empty() || options.info.version.trim().is_empty() {
        issues.push(issue(true, Message::new("pack.check.info")));
    }

    for id in &options.profiles {
        let profile = profiles.get(id)?;
        let listed = crate::library::mods_of(profile, &library, &game);

        for entry in &profile.mods {
            let name = library
                .families
                .get(&entry.id)
                .map_or_else(|| entry.id.clone(), |family| family.name.clone());
            let record = library.versions.get(&entry.folder);
            let archive = record
                .and_then(|record| record.archive.as_ref())
                .and_then(|sha256| library.archive_file_of(sha256))
                .filter(|file| data.archives().join(file).is_file());
            let has_source = library
                .families
                .get(&entry.id)
                .is_some_and(|family| !family.sources.is_empty());
            let travels = archive.is_some() && options.archives != PackArchives::None;

            if !has_source && !travels {
                issues.push(issue(
                    true,
                    Message::new("pack.check.unshareable").with("mod", &name),
                ));
            } else if record.is_some_and(|record| record.archive.is_none()) {
                issues.push(issue(
                    false,
                    Message::new("pack.check.no_archive").with("mod", &name),
                ));
            }
        }
        // Check the pack form too: a group whose members fall outside the mod list cannot be rebuilt.
        let pack = super::export::pack_profile(&data, &library, &steps, profile);

        for problem in group_issues(&pack) {
            issues.push(issue(
                problem.error,
                problem.message.with("profile", &profile.name),
            ));
        }

        issues.extend(
            relation_warnings(&library, &view, &listed)
                .into_iter()
                .map(|message| issue(false, message.with("profile", &profile.name))),
        );

        if is_ready(
            &data,
            &game,
            &settings,
            profile,
            &data.profile_state(&profile.id),
        ) {
            let inputs = library.inputs(&listed);

            if let Ok(plan) = build_plan_with_mode(
                &data,
                &game,
                &settings,
                profile,
                &profile.winners,
                &inputs,
                PlanMode::Summary,
            ) {
                let open = plan
                    .conflicts
                    .iter()
                    .filter(|conflict| {
                        !profile
                            .winners
                            .contains_key(&crate::domain::pathkey::path_key(&conflict.path))
                    })
                    .count();
                if open > 0 {
                    issues.push(issue(
                        false,
                        Message::new("pack.check.conflicts")
                            .with("count", open)
                            .with("profile", &profile.name),
                    ));
                }
            }
        }
    }

    Ok(issues)
}

/// Warns about mod groups whose member indices fall outside the pack's mod list. Such an index can come from a
/// hand-edited manifest; the import skips it instead of indexing past the list, and this reports the damage.
pub(super) fn group_issues(profile: &PackProfile) -> Vec<PackIssue> {
    let count = profile.mods.len();
    profile
        .groups
        .iter()
        .filter(|group| group.members.iter().any(|&index| index >= count))
        .map(|group| PackIssue {
            error: false,
            message: Message::new("pack.check.group_member").with("group", &group.name),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_with_group(members: &[usize]) -> PackProfile {
        // Take the kind's wire form from the type itself: the check only cares about the member indices.
        let kind = serde_json::to_value(crate::domain::profiles::GroupKind::Bundle).unwrap();
        serde_json::from_value(serde_json::json!({
            "name": "Test",
            "mods": [
                { "name": "mod-a", "version": null, "family": "a", "enabled": true },
                { "name": "mod-b", "version": null, "family": "b", "enabled": true }
            ],
            "groups": [{ "name": "Core Set", "kind": kind, "members": members }]
        }))
        .unwrap()
    }

    #[test]
    fn a_group_with_an_out_of_range_member_is_reported_without_panicking() {
        let profile = profile_with_group(&[0, 7]);
        let issues = group_issues(&profile);
        assert_eq!(issues.len(), 1);
        assert!(!issues[0].error);
        assert_eq!(issues[0].message.code, "pack.check.group_member");
    }

    #[test]
    fn a_group_with_only_valid_members_is_clean() {
        assert!(group_issues(&profile_with_group(&[0, 1])).is_empty());
    }
}
