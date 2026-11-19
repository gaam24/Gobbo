//! FOMOD installers: `fomod/ModuleConfig.xml` describes the steps, groups and install options, and the chosen
//! options point to the files in the archive to extract. We support `installSteps` with group types
//! (`SelectAny`, `SelectAtLeastOne`, `SelectExactlyOne`, `SelectAtMostOne`, `SelectAll`), option types
//! (`Required`, `Optional`, `Recommended`, `NotUsable`), `conditionFlags`, and
//! `conditionalFileInstalls` with file and flag dependencies (And/Or). `requiredInstallFiles`,
//! `alwaysInstall`, `installIfUsable` and `priority` are honored without a choice; a `<dependencyType>` picks the
//! effective option kind from the flags and installed files set before the option. The wizard asks the same walk
//! (`installer_state`), so it shows that kind too. `moduleDependencies` are checked before any step. We treat `gameDependency` as met, because Gobbo
//! does not know the game version at import time, and ignore `fommDependency`. The place the files end up in
//! the game is decided by the game's layout (`slots`/`mirror`) anyway: FOMOD only says what to pack.

mod errors;
mod model;
mod parse;
mod sources;
mod state;
mod text;

pub use model::{Choice, Condition, Dependencies, Dependency, GroupKind, OptionKind};
pub use parse::parse;
#[cfg(test)]
pub use sources::matches_source;
/// The allocating form is only used by the benchmark fixture; an import uses `normalize_path_into` with a buffer.
#[cfg(feature = "bench")]
pub use sources::normalize_path;
pub use sources::{SourcePattern, normalize_path_into, resolve_sources};
pub use state::{FomodState, installer_state};
pub use text::decode;

#[cfg(test)]
mod tests;
