//! The deployment plan. The function is pure and does not touch the disk. The order of the list decides conflicts.
//!
//! Every file of an enabled mod gets one destination, relative to the deployment root (`domain::deploy::Deploy`):
//! - a file under a mirrored folder keeps its game path (`archive/pc/mod/a.archive`, `DLC/dlcfoo/x.bundle`);
//! - a file of a mod in a slot goes into the mod's own folder there (`Mods/modFoo/content/a.ws`), or straight into the
//!   slot folder when the slot is flat (`Content/Paks/~mods/a.pak`);
//! - a mod without a slot keeps game paths for all its files.
//!
//! Two mods meet at a game path (a mirror or a flat slot), and at a shared kind of file
//! (`domain::deploy::Shared`) that the game reads from one mod only. Of those, only the winner's copy is deployed: the
//! one the user chose, else the first on the list. Files in a mod's own folder never meet another mod's. Only mods with
//! a priority the user set, and a folder of their own (`is_orderable`), get a row in the game's order file.

mod model;
mod own_folder;
mod planner;
mod shared;

#[cfg(test)]
pub use crate::domain::layout::Materialized;
#[cfg(all(test, windows))]
pub use model::PlanSource;
#[cfg(test)]
pub use model::PlannedFile;
pub use model::{Conflict, DeployPlan, ModInput, PlanMode, Problem, SettingsRow};
pub use own_folder::is_orderable;
pub use planner::LayoutRules;
pub use planner::{layout_profile, plan_with_mode};

#[cfg(test)]
mod tests;
