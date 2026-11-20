use rustc_hash::{FxHashMap, FxHashSet};

use super::super::model::{Condition, Dependencies, Dependency, FomodOption, OptionKind};
use super::pattern::normalize_path;

/// The kind the resolver uses for an option. A `<dependencyType>` takes the kind of its first pattern whose
/// dependencies hold in the flags and installed destinations of the earlier steps, or its default kind when none match.
/// Without one, the parse-time kind is used.
pub(super) fn effective_kind(
    option: &FomodOption,
    flags: &FxHashMap<String, String>,
    installed: &FxHashSet<String>,
) -> OptionKind {
    let Some(dependency_type) = &option.dependency_type else {
        return option.kind;
    };

    for pattern in &dependency_type.patterns {
        if satisfied(&pattern.dependencies, flags, installed) {
            return pattern.kind;
        }
    }

    dependency_type.default_kind
}

/// Whether an install step is shown: no `<visible>` condition means always. The walk calls it for every step, so the
/// wizard and the resolver hide a step at the same time.
pub(in crate::import::fomod) fn step_visible(
    visible: Option<&Dependencies>,
    flags: &FxHashMap<String, String>,
    installed: &FxHashSet<String>,
) -> bool {
    visible.is_none_or(|dependencies| satisfied(dependencies, flags, installed))
}

pub(in crate::import::fomod) fn satisfied(
    dependencies: &Dependencies,
    flags: &FxHashMap<String, String>,
    installed: &FxHashSet<String>,
) -> bool {
    match dependencies {
        Dependencies::All(children) => children
            .iter()
            .all(|child| matches_condition(child, flags, installed)),
        Dependencies::Any(children) => children
            .iter()
            .any(|child| matches_condition(child, flags, installed)),
    }
}

fn matches_condition(
    condition: &Condition,
    flags: &FxHashMap<String, String>,
    installed: &FxHashSet<String>,
) -> bool {
    match condition {
        Condition::Dependency(dependency) => matches_one(dependency, flags, installed),
        Condition::Group(group) => satisfied(group, flags, installed),
    }
}

fn matches_one(
    dependency: &Dependency,
    flags: &FxHashMap<String, String>,
    installed: &FxHashSet<String>,
) -> bool {
    match dependency {
        Dependency::File { file, active } => {
            let normalized = normalize_path(file);
            let is_installed = installed.contains(&normalized);
            *active == is_installed
        }
        Dependency::Flag { flag, value } => flags
            .get(flag.as_str())
            .is_some_and(|found| found.eq_ignore_ascii_case(value)),
        Dependency::Game => true,
    }
}
