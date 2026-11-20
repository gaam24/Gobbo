//! Which files of the archive an installer's choices select, and how a source pattern matches an entry.

use rustc_hash::{FxHashMap, FxHashSet};

use super::errors::FomodError;
use super::model::{Choice, ModuleConfig, Pattern};

mod choices;
mod conditions;
mod pattern;
mod walk;

pub(super) use choices::{chosen_by_group, named_picks};
pub(super) use conditions::satisfied;
#[cfg(test)]
pub(super) use conditions::step_visible;
#[cfg(test)]
pub use pattern::matches_source;
#[cfg(feature = "bench")]
pub use pattern::normalize_path;
pub use pattern::{SourcePattern, normalize_path_into};
pub(super) use walk::run_installer;

use choices::check_choices;

/// Finds the options that match the choices: required options always go in, the rest must exist
/// and satisfy the group rules. Returns the file patterns from the selected options plus the conditional ones whose
/// dependencies are met by the choice flags and the install tree. `<requiredInstallFiles>`, `alwaysInstall` and
/// `installIfUsable` files go in without a choice; the result is ordered by `priority`, so a higher priority is
/// resolved later and wins on an equal destination.
pub fn resolve_sources(
    config: &ModuleConfig,
    choices: &[Choice],
) -> Result<Vec<Pattern>, FomodError> {
    // `<moduleDependencies>` are checked before any step, with no flags and no installed files: nothing can be set
    // or installed before the first step. A `gameDependency` is always met (the game version is unknown at import
    // time) and a `fommDependency` is ignored by the parser.
    let no_flags = FxHashMap::default();
    let no_installed = FxHashSet::default();

    if config
        .module_dependencies
        .as_ref()
        .is_some_and(|dependencies| !satisfied(dependencies, &no_flags, &no_installed))
    {
        return Err(FomodError::ModuleDependenciesUnmet);
    }

    check_choices(config, choices)?;
    let chosen = chosen_by_group(config, choices);
    let run = run_installer(config, |step, group_index, group, _| {
        named_picks(&chosen, step, group_index, group)
    });

    // The first visible group that breaks its rule, in XML order, is the error.
    for (config_step, step) in config.steps.iter().zip(&run.steps) {
        if !step.visible {
            continue;
        }

        for (config_group, group) in config_step.groups.iter().zip(&step.groups) {
            if let Some(problem) = group.problem {
                return Err(FomodError::GroupRule {
                    group: config_group.name.clone(),
                    rule: problem.rule(group.count),
                });
            }
        }
    }

    // A higher priority is resolved later, so it wins when two patterns share a destination. The sort is stable:
    // patterns with the same priority keep the order in which the XML produced them.
    let mut sources = run.sources;
    sources.sort_by_key(|pattern| pattern.priority);
    Ok(sources)
}
