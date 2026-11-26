//! What the FOMOD wizard shows for an installer under the user's picks: which steps are shown, which groups break their
//! rule, the kind of every option and the picks the import takes. It reads the same walk as the resolver
//! (`sources::run_installer`), so the wizard and the import cannot disagree about a step, a group or an option.

use serde::Serialize;
use specta::Type;

use super::model::{Group, GroupKind, GroupProblem, ModuleConfig, OptionKind};
use super::sources::{chosen_by_group, named_picks, run_installer};
use crate::domain::fomod::Choice;

/// The wizard's answer for one selection.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FomodState {
    /// The indices of the steps the import shows, in XML order.
    pub visible: Vec<u32>,
    /// One entry per step of the installer, in XML order. A hidden step is listed too, with nothing picked.
    pub steps: Vec<FomodStepState>,
    /// The picks the import takes from the wizard: the chosen options of the visible steps, without the required and
    /// the unusable ones. The wizard sends it back as the selection of the install.
    pub selection: Vec<Choice>,
    /// How many options of the visible steps the import installs, required ones included.
    pub picked: u32,
}

/// One step of the wizard's answer, with the state of each of its groups.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FomodStepState {
    /// One entry per group of the step, in XML order.
    pub groups: Vec<FomodGroupState>,
}

/// One group of the wizard's answer: its rule, any broken rule and the state of each option.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FomodGroupState {
    /// The group's name, so a click can name the option without another round trip to the installer.
    pub name: String,
    /// The rule of the group. The pick rules read this, not a copy in the window.
    pub kind: GroupKind,
    /// The rule the group breaks, or `None` when it is fine (or the step is hidden).
    pub problem: Option<GroupProblem>,
    /// One entry per option of the group, in XML order.
    pub options: Vec<FomodOptionState>,
}

/// One option of the wizard's answer: its effective kind and whether the import installs it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct FomodOptionState {
    /// The option's name, so a click can be checked against the option the installer still has.
    pub name: String,
    /// The kind the import applies: a `<dependencyType>` can make an option required or unusable.
    pub kind: OptionKind,
    /// Whether the import installs the option under this selection: required, chosen, or part of a `SelectAll` group.
    pub picked: bool,
}

/// The wizard's answer for an installer. `None` is the default selection: the recommended and required options, the
/// first usable option of an exactly-one group that has none, and every usable option of a `SelectAll` group. A `Some`
/// selection is taken as the user left it; a choice that the installer does not define is ignored.
pub fn installer_state(config: &ModuleConfig, selection: Option<&[Choice]>) -> FomodState {
    let run = selection.map_or_else(
        || run_installer(config, |_, _, group, kinds| default_picks(group, kinds)),
        |choices| {
            let chosen = chosen_by_group(config, choices);
            run_installer(config, |step, group_index, group, _| {
                named_picks(&chosen, step, group_index, group)
            })
        },
    );

    let mut visible = Vec::new();
    let mut installed = 0usize;
    let mut selection_out = Vec::new();
    let mut steps = Vec::with_capacity(run.steps.len());

    for (step_index, (config_step, step)) in config.steps.iter().zip(&run.steps).enumerate() {
        if step.visible {
            visible.push(to_u32(step_index));
        }

        let mut groups = Vec::with_capacity(step.groups.len());

        for (group_index, (config_group, group)) in
            config_step.groups.iter().zip(&step.groups).enumerate()
        {
            let rows = config_group
                .options
                .iter()
                .zip(&group.kinds)
                .zip(&group.picked)
                .zip(&group.chosen);
            let mut options = Vec::with_capacity(group.kinds.len());

            for (((option, kind), picked), chosen) in rows {
                if step.visible {
                    if *picked && *kind != OptionKind::NotUsable {
                        installed += 1;
                    }

                    if *chosen {
                        selection_out.push(Choice {
                            step: to_u32(step_index),
                            group_index: to_u32(group_index),
                            group: config_group.name.clone(),
                            option: option.name.clone(),
                        });
                    }
                }

                options.push(FomodOptionState {
                    name: option.name.clone(),
                    kind: *kind,
                    picked: *picked,
                });
            }

            groups.push(FomodGroupState {
                name: config_group.name.clone(),
                kind: config_group.kind,
                problem: group.problem,
                options,
            });
        }

        steps.push(FomodStepState { groups });
    }

    FomodState {
        visible,
        steps,
        selection: selection_out,
        picked: to_u32(installed),
    }
}

impl FomodState {
    /// The selection after one click, from the rules the import applies. The wizard sends its current answer and the
    /// click; the backend decides the next selection so the window only renders the answer.
    pub fn toggle_pick(&self, step: u32, group: u32, option: &str) -> Vec<Choice> {
        toggle_pick(self, step, group, option)
    }

    /// The selection a reinstall starts from: saved answers laid over the defaults in this answer.
    pub fn selection_from_choices(&self, saved: &[Choice]) -> Vec<Choice> {
        selection_from_choices(self, saved)
    }
}

/// The selection after a click on one option of the wizard. `view` is the backend's answer for the current selection:
/// it carries the visible steps, each group's rule and the effective kinds of its options, so the rules use the same
/// data as the import. A click on a hidden step, a `SelectAll` group, or an option installed without a choice (required
/// or unusable) leaves the selection alone. A group that takes one option keeps only the clicked one; an at-most-one
/// group is cleared by a click on its own pick.
pub fn toggle_pick(view: &FomodState, step: u32, group: u32, option: &str) -> Vec<Choice> {
    if !view.visible.contains(&step) {
        return view.selection.clone();
    }

    let Some(step_index) = usize::try_from(step).ok() else {
        return view.selection.clone();
    };
    let Some(group_index) = usize::try_from(group).ok() else {
        return view.selection.clone();
    };
    let Some(group_view) = view
        .steps
        .get(step_index)
        .and_then(|step| step.groups.get(group_index))
    else {
        return view.selection.clone();
    };
    let Some(kind) = group_view
        .options
        .iter()
        .find(|entry| entry.name == option)
        .map(|entry| entry.kind)
    else {
        return view.selection.clone();
    };

    if group_view.kind == GroupKind::SelectAll
        || matches!(kind, OptionKind::Required | OptionKind::NotUsable)
    {
        return view.selection.clone();
    }

    let choice = Choice {
        step,
        group_index: group,
        group: group_view.name.clone(),
        option: option.to_string(),
    };
    let same_group = |entry: &Choice| entry.step == step && entry.group_index == group;
    let picked = view
        .selection
        .iter()
        .any(|entry| same_group(entry) && entry.option == option);
    match group_view.kind {
        GroupKind::SelectExactlyOne | GroupKind::SelectAtMostOne => {
            let mut next: Vec<Choice> = view
                .selection
                .iter()
                .filter(|entry| !same_group(entry))
                .cloned()
                .collect();
            if group_view.kind != GroupKind::SelectAtMostOne || !picked {
                next.push(choice);
            }

            next
        }
        GroupKind::SelectAny | GroupKind::SelectAtLeastOne => {
            if picked {
                view.selection
                    .iter()
                    .filter(|entry| !(same_group(entry) && entry.option == option))
                    .cloned()
                    .collect()
            } else {
                let mut next = view.selection.clone();
                next.push(choice);
                next
            }
        }
        GroupKind::SelectAll => view.selection.clone(),
    }
}

/// The selection a reinstall starts from: the saved answers laid over the defaults in `view`, with the same validation
/// as a click. A saved answer whose step is hidden, whose option the installer no longer has, or which the import
/// installs without a choice is dropped; the group name is taken from the view, so a renamed group still matches. In a
/// group that takes one option the last saved answer wins.
pub fn selection_from_choices(view: &FomodState, saved: &[Choice]) -> Vec<Choice> {
    let mut selection = view.selection.clone();

    for choice in saved {
        if !view.visible.contains(&choice.step) {
            continue;
        }

        let Some(step_index) = usize::try_from(choice.step).ok() else {
            continue;
        };
        let Some(group_index) = usize::try_from(choice.group_index).ok() else {
            continue;
        };
        let Some(group_view) = view
            .steps
            .get(step_index)
            .and_then(|step| step.groups.get(group_index))
        else {
            continue;
        };
        let Some(kind) = group_view
            .options
            .iter()
            .find(|entry| entry.name == choice.option)
            .map(|entry| entry.kind)
        else {
            continue;
        };

        if matches!(kind, OptionKind::Required | OptionKind::NotUsable) {
            continue;
        }

        let chosen = Choice {
            step: choice.step,
            group_index: choice.group_index,
            group: group_view.name.clone(),
            option: choice.option.clone(),
        };
        let same_group =
            |entry: &Choice| entry.step == choice.step && entry.group_index == choice.group_index;
        match group_view.kind {
            GroupKind::SelectExactlyOne | GroupKind::SelectAtMostOne => {
                selection.retain(|entry| !same_group(entry));
                selection.push(chosen);
            }
            GroupKind::SelectAny | GroupKind::SelectAtLeastOne => {
                if !selection
                    .iter()
                    .any(|entry| same_group(entry) && entry.option == choice.option)
                {
                    selection.push(chosen);
                }
            }
            GroupKind::SelectAll => {}
        }
    }

    selection
}

/// The default picks of one group, from its effective kinds. A required option is installed and counted by the import
/// without a pick; in a group that takes at most one option it already fills the slot, so no recommended option is
/// added on top of it. An exactly-one group falls back to the first usable option when the author recommends none.
fn default_picks(group: &Group, kinds: &[OptionKind]) -> Vec<bool> {
    let usable = |kind: &OptionKind| *kind != OptionKind::NotUsable;

    match group.kind {
        GroupKind::SelectAll => kinds.iter().map(usable).collect(),
        GroupKind::SelectExactlyOne | GroupKind::SelectAtMostOne => {
            let mut picks = vec![false; kinds.len()];
            let choice = kinds
                .iter()
                .position(|kind| *kind == OptionKind::Required)
                .or_else(|| {
                    kinds
                        .iter()
                        .position(|kind| *kind == OptionKind::Recommended)
                });
            let choice = if group.kind == GroupKind::SelectExactlyOne {
                choice.or_else(|| kinds.iter().position(usable))
            } else {
                choice
            };

            if let Some(index) = choice
                && let Some(pick) = picks.get_mut(index)
            {
                *pick = true;
            }

            picks
        }
        GroupKind::SelectAtLeastOne | GroupKind::SelectAny => kinds
            .iter()
            .map(|kind| matches!(kind, OptionKind::Recommended | OptionKind::Required))
            .collect(),
    }
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
