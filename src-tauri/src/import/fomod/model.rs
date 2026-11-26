//! What a FOMOD installer says: its steps, groups, options and the conditions between them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

/// A parsed `ModuleConfig.xml` in a form that can resolve the choices and conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleConfig {
    pub module_name: String,
    /// Files from `<requiredInstallFiles>`. They are installed before any step, without a choice.
    pub required_files: Vec<Pattern>,
    pub steps: Vec<Step>,
    /// The `<moduleDependencies>` checked before any step, when the installer has them.
    pub module_dependencies: Option<Dependencies>,
    /// Conditional files from `conditionalFileInstalls`, in the order of the XML.
    pub conditional: Vec<Conditional>,
}

/// One `<step>` of the installer wizard: the groups the user picks from and when the step is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: String,
    /// The step's `<visible>` condition. `None` means the step is always shown.
    pub visible: Option<Dependencies>,
    pub groups: Vec<Group>,
}

/// One `<group>` of a step: its name, selection rule and the options the user picks from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub kind: GroupKind,
    pub options: Vec<FomodOption>,
}

/// How many options a group lets the user pick, from the `type` attribute of `<group>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
#[expect(
    clippy::enum_variant_names,
    reason = "the names mirror the `type` attribute of `group` in ModuleConfig.xml, spec for spec, so the variants mirror the installer format rather than an internal naming choice"
)]
pub enum GroupKind {
    SelectAny,
    SelectAtLeastOne,
    SelectExactlyOne,
    SelectAtMostOne,
    SelectAll,
}

/// One `<option>` of a group, with its files, flags and the rule that decides whether it is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FomodOption {
    pub name: String,
    pub description: String,
    /// The parse-time kind: a plain `<type>`, or the `<defaultType>` of a `<dependencyType>`. The walk applies the
    /// effective kind, which `dependency_type` can change, so this is only the kind before any flag is known.
    pub kind: OptionKind,
    /// File patterns to extract when the option is selected.
    pub files: Vec<Pattern>,
    /// Flags for `flagDependency`, for example "variant" = "hd".
    pub flags: BTreeMap<String, String>,
    /// The parsed `<dependencyType>`, when the option's kind depends on earlier flags and installed files.
    /// The walk evaluates this against the flags and installed files set before the option, for the wizard and the import.
    pub dependency_type: Option<DependencyType>,
}

/// A parsed `<typeDescriptor><dependencyType>`: a default kind and ordered patterns that can replace it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyType {
    pub default_kind: OptionKind,
    pub patterns: Vec<TypePattern>,
}

/// One `<pattern>` of a `<dependencyType>`: when `dependencies` hold, `kind` replaces the default kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypePattern {
    pub dependencies: Dependencies,
    pub kind: OptionKind,
}

/// The kind an option is declared with in the installer, before a `<dependencyType>` can replace it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OptionKind {
    Required,
    Optional,
    Recommended,
    NotUsable,
    CouldBeUsable,
}

/// The rule a visible group breaks, as the wizard shows it. The import turns the same problem into its message code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
#[expect(
    clippy::enum_variant_names,
    reason = "the codes mirror the wizard's group rules: pick one, at least one, at most one"
)]
pub enum GroupProblem {
    /// A `SelectExactlyOne` group does not have exactly one pick.
    PickOne,
    /// A `SelectAtLeastOne` group has no pick.
    PickAtLeastOne,
    /// A `SelectAtMostOne` group has more than one pick.
    PickAtMostOne,
}

impl GroupProblem {
    /// The message code of the import for this problem. `picked` counts the picks that count toward the rule, required
    /// options included.
    pub(super) const fn rule(self, picked: usize) -> &'static str {
        match self {
            Self::PickOne if picked == 0 => "fomod.group_pick_exactly",
            Self::PickOne => "fomod.group_pick_only",
            Self::PickAtLeastOne => "fomod.group_pick_at_least",
            Self::PickAtMostOne => "fomod.group_pick_at_most",
        }
    }
}

/// A file or folder from the archive: `source` with an optional `*`, `destination` in the install tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub source: String,
    pub destination: String,
    /// `alwaysInstall`: install even when the option is not selected.
    pub always_install: bool,
    /// `installIfUsable`: install when the option's effective kind is not `NotUsable`, even when not selected.
    pub install_if_usable: bool,
    /// `priority`: higher values are resolved later and therefore win on an equal destination.
    pub priority: i32,
}

/// Files installed when a `<conditionalFileInstalls>` dependency holds, in the order of the XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conditional {
    pub dependencies: Dependencies,
    pub files: Vec<Pattern>,
}

/// The dependency tree from the `And`/`Or` operator in `dependencies operator=...` (And by default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dependencies {
    All(Vec<Condition>),
    Any(Vec<Condition>),
}

/// One child of a dependency group: a leaf check or a nested group with its own operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    Dependency(Dependency),
    Group(Dependencies),
}

/// One leaf check of a dependency tree: a file's state, an option's flag or the game version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dependency {
    /// A file in the install tree is active or inactive.
    File { file: String, active: bool },
    /// The flag from `conditionFlags` of the selected option has this value.
    Flag { flag: String, value: String },
    /// Game version — treated as met.
    Game,
}
