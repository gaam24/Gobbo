use std::collections::HashMap;

use super::super::errors::FomodError;
use super::super::model::{Group, GroupKind, GroupProblem, ModuleConfig};
use crate::domain::fomod::Choice;

/// Finds the group a choice belongs to. The indices decide; a record written before the indices existed has zeroes
/// and falls back to a group name that appears exactly once.
fn chosen_group<'a>(
    config: &'a ModuleConfig,
    choice: &Choice,
) -> Option<(usize, usize, &'a Group)> {
    let step_index = usize::try_from(choice.step).ok()?;
    let group_index = usize::try_from(choice.group_index).ok()?;

    if let Some(step) = config.steps.get(step_index)
        && let Some(group) = step.groups.get(group_index)
        && group.name == choice.group
    {
        return Some((step_index, group_index, group));
    }
    // Only the first position can be a record from before the indices: every entry then has zeroes.
    if choice.step != 0 || choice.group_index != 0 {
        return None;
    }

    let mut found = None;

    for (step_index, step) in config.steps.iter().enumerate() {
        for (group_index, group) in step.groups.iter().enumerate() {
            if group.name == choice.group {
                // Two groups share this name, so the old name-only record cannot say which one it meant.
                if found.is_some() {
                    return None;
                }

                found = Some((step_index, group_index, group));
            }
        }
    }

    found
}

/// Checks that every choice names an option the installer defines. The wizard should not be able to send anything else.
/// Choices are keyed by the group's position, never by its name: two groups may share a name.
pub(super) fn check_choices(config: &ModuleConfig, choices: &[Choice]) -> Result<(), FomodError> {
    for choice in choices {
        let Some((_, _, group)) = chosen_group(config, choice) else {
            return Err(FomodError::UnknownOption {
                group: choice.group.clone(),
                option: choice.option.clone(),
            });
        };

        if !group
            .options
            .iter()
            .any(|option| option.name == choice.option)
        {
            return Err(FomodError::UnknownOption {
                group: group.name.clone(),
                option: choice.option.clone(),
            });
        }
    }

    Ok(())
}

/// The chosen option names of each group, keyed by the group's position. A choice that names no option of the
/// installer is left out; `check_choices` is what refuses it for the import.
pub(in crate::import::fomod) fn chosen_by_group<'a>(
    config: &ModuleConfig,
    choices: &'a [Choice],
) -> HashMap<(usize, usize), Vec<&'a str>> {
    let mut chosen: HashMap<(usize, usize), Vec<&'a str>> = HashMap::new();

    for choice in choices {
        let Some((step, group_index, group)) = chosen_group(config, choice) else {
            continue;
        };

        if !group
            .options
            .iter()
            .any(|option| option.name == choice.option)
        {
            continue;
        }

        let names = chosen.entry((step, group_index)).or_default();

        if !names.contains(&choice.option.as_str()) {
            names.push(choice.option.as_str());
        }
    }

    chosen
}

/// The options a selection names in one group, as one flag per option of the group.
pub(in crate::import::fomod) fn named_picks(
    chosen: &HashMap<(usize, usize), Vec<&str>>,
    step: usize,
    group_index: usize,
    group: &Group,
) -> Vec<bool> {
    let names = chosen.get(&(step, group_index));
    group
        .options
        .iter()
        .map(|option| names.is_some_and(|names| names.contains(&option.name.as_str())))
        .collect()
}

/// The rule a group breaks, given how many picks count toward it. Required options count, as the installer applies them.
pub(super) const fn group_problem(kind: GroupKind, count: usize) -> Option<GroupProblem> {
    match kind {
        GroupKind::SelectExactlyOne if count != 1 => Some(GroupProblem::PickOne),
        GroupKind::SelectAtLeastOne if count == 0 => Some(GroupProblem::PickAtLeastOne),
        GroupKind::SelectAtMostOne if count > 1 => Some(GroupProblem::PickAtMostOne),
        _ => None,
    }
}
