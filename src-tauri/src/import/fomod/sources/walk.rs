use rustc_hash::{FxHashMap, FxHashSet};

use super::super::model::{Group, GroupKind, GroupProblem, ModuleConfig, OptionKind, Pattern};
use super::choices::group_problem;
use super::conditions::{effective_kind, satisfied, step_visible};
use super::pattern::normalize_path;

/// One walk of the installer under one selection. The resolver and the wizard state both read it, so they cannot
/// disagree about a step, a group or an option.
pub(in crate::import::fomod) struct InstallerRun {
    /// The patterns that install, in the order the walk produced them (before the sort by priority).
    pub(in crate::import::fomod) sources: Vec<Pattern>,
    /// One entry per step, in XML order.
    pub(in crate::import::fomod) steps: Vec<StepRun>,
}

pub(in crate::import::fomod) struct StepRun {
    /// Whether the step is shown. A hidden step has no problems, sets no flags and installs no files.
    pub(in crate::import::fomod) visible: bool,
    pub(in crate::import::fomod) groups: Vec<GroupRun>,
}

pub(in crate::import::fomod) struct GroupRun {
    /// The rule the group breaks, when the group is visible and breaks it.
    pub(in crate::import::fomod) problem: Option<GroupProblem>,
    /// The picks that count toward the rule: required options and the chosen usable ones.
    pub(in crate::import::fomod) count: usize,
    /// The effective kind of each option, the kind the import applies.
    pub(in crate::import::fomod) kinds: Vec<OptionKind>,
    /// Whether the import installs each option: required, chosen, or any option of a `SelectAll` group.
    pub(in crate::import::fomod) picked: Vec<bool>,
    /// The options the selection takes from the wizard: chosen, and neither required nor unusable.
    pub(in crate::import::fomod) chosen: Vec<bool>,
}

/// The effective kinds a group's options end up with once the options the selection takes have set their flags and
/// added their files. This is a dry run of the option loop below: the real flags and install tree are left untouched,
/// so the caller can answer its picks from the same kinds the import will apply to the group.
fn final_group_kinds(
    group: &Group,
    named: &[bool],
    flags: &FxHashMap<String, String>,
    destinations: &FxHashSet<String>,
) -> Vec<OptionKind> {
    let mut flags = flags.clone();
    let mut destinations = destinations.clone();
    let mut kinds = Vec::with_capacity(group.options.len());
    for (option, named) in group.options.iter().zip(named) {
        let kind = effective_kind(option, &flags, &destinations);
        let usable = kind != OptionKind::NotUsable;
        let installs = group.kind == GroupKind::SelectAll || kind == OptionKind::Required || *named;
        if installs && usable {
            for flag in &option.flags {
                flags.insert(flag.0.clone(), flag.1.clone());
            }
        }
        for pattern in &option.files {
            let include = (installs && usable)
                || pattern.always_install
                || (pattern.install_if_usable && usable);
            if include {
                destinations.insert(normalize_path(&pattern.destination));
            }
        }
        kinds.push(kind);
    }
    kinds
}

/// Walks the installer in XML order, the way the import applies it. `picks` answers which options the selection takes
/// in one group, for a visible step; it gets the step and group positions, the group and the effective kinds of the
/// group — first with no flags from the group itself, then with the kinds its first answer produces — and returns one
/// flag per option. Resolving the group's own flag effects before its options are read keeps the wizard's defaults and
/// the import's rule count on the same kinds; the options are then read in order, as the import always did.
pub(in crate::import::fomod) fn run_installer(
    config: &ModuleConfig,
    mut picks: impl FnMut(usize, usize, &Group, &[OptionKind]) -> Vec<bool>,
) -> InstallerRun {
    let mut sources = Vec::new();
    let mut flags: FxHashMap<String, String> = FxHashMap::default();
    let mut destinations: FxHashSet<String> = FxHashSet::default();

    // `<requiredInstallFiles>` go in first and need no choice. They also count for `<visible>` and conditional file
    // dependencies, like the files of the steps below.
    for pattern in &config.required_files {
        destinations.insert(normalize_path(&pattern.destination));
        sources.push(pattern.clone());
    }

    let mut steps = Vec::with_capacity(config.steps.len());
    for (step_index, step) in config.steps.iter().enumerate() {
        // `<visible>` is evaluated from the state the earlier steps leave: nothing this step sets is seen by it.
        let visible = step_visible(step.visible.as_ref(), &flags, &destinations);
        let mut groups = Vec::with_capacity(step.groups.len());
        for (group_index, group) in step.groups.iter().enumerate() {
            let named = if visible {
                let start_kinds: Vec<OptionKind> = group
                    .options
                    .iter()
                    .map(|option| effective_kind(option, &flags, &destinations))
                    .collect();
                let candidate = picks(step_index, group_index, group, &start_kinds);
                // A picked option can set a flag that makes a later option of this group required or usable. Answer
                // once more against those resolved kinds so a default that picks the flag-setting option does not
                // leave a required option the rule also counts. Iterate only once: re-resolving the new answer can
                // oscillate when the first pick is the very flag that changes the later option's kind.
                let resolved = final_group_kinds(group, &candidate, &flags, &destinations);
                picks(step_index, group_index, group, &resolved)
            } else {
                vec![false; group.options.len()]
            };

            let mut kinds = Vec::with_capacity(group.options.len());
            let mut picked = Vec::with_capacity(group.options.len());
            let mut chosen = Vec::with_capacity(group.options.len());
            for (option, named) in group.options.iter().zip(&named) {
                let kind = effective_kind(option, &flags, &destinations);
                let usable = kind != OptionKind::NotUsable;
                // `SelectAll` installs every option, so there is nothing for the user to pick.
                let installs = visible
                    && (group.kind == GroupKind::SelectAll
                        || kind == OptionKind::Required
                        || *named);
                // A required option is not a choice, and an unusable one cannot be chosen: neither counts toward the rule.
                chosen.push(*named && visible && kind != OptionKind::Required && usable);
                if visible {
                    // Only a usable, picked option sets flags. An option the user did not pick still contributes its
                    // always-install and install-if-usable files.
                    if installs && usable {
                        for flag in &option.flags {
                            flags.insert(flag.0.clone(), flag.1.clone());
                        }
                    }
                    for pattern in &option.files {
                        let include = (installs && usable)
                            || pattern.always_install
                            || (pattern.install_if_usable && usable);
                        if !include {
                            continue;
                        }
                        destinations.insert(normalize_path(&pattern.destination));
                        sources.push(pattern.clone());
                    }
                }
                kinds.push(kind);
                // An unusable option is never installed, whatever the group or the selection says.
                picked.push(installs && usable);
            }

            let required = kinds
                .iter()
                .filter(|kind| **kind == OptionKind::Required)
                .count();
            let count = required + chosen.iter().filter(|chosen| **chosen).count();
            let problem = if visible {
                group_problem(group.kind, count)
            } else {
                None
            };
            groups.push(GroupRun {
                problem,
                count,
                kinds,
                picked,
                chosen,
            });
        }
        steps.push(StepRun { visible, groups });
    }

    // Conditional files: dependencies are checked against the flags of the selected options and the install tree
    // from the files that are already going in. Patterns count in the order of the XML.
    for conditional in &config.conditional {
        if satisfied(&conditional.dependencies, &flags, &destinations) {
            for pattern in &conditional.files {
                destinations.insert(normalize_path(&pattern.destination));
                sources.push(pattern.clone());
            }
        }
    }
    InstallerRun { sources, steps }
}
