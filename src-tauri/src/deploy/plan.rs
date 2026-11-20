//! Building a deployment plan from the enabled mods and their files. The plan is pure: the overlay never writes the
//! game folder, so no file or folder there can block it.

use std::collections::BTreeMap;
#[cfg(test)]
use std::path::Path;

use crate::domain::resolver::{self, DeployPlan, ModInput, PlanMode};
use crate::games::Deploy;

#[cfg(test)]
use super::DeployError;
#[cfg(test)]
use super::scan;

/// Builds the plan from ready data about mods: files from the library index or from a scan. It does not read the
/// library itself.
pub fn plan(deploy: &Deploy, mods: &[ModInput], winners: &BTreeMap<String, String>) -> DeployPlan {
    plan_with_mode(deploy, mods, winners, PlanMode::Full)
}

/// Builds the plan in `Full` or `Summary` mode. `Summary` skips the files that cannot conflict and do not leave the
/// mod's own folder; the status and the preview only need the count, the conflicts and the problems.
pub fn plan_with_mode(
    deploy: &Deploy,
    mods: &[ModInput],
    winners: &BTreeMap<String, String>,
    plan_mode: PlanMode,
) -> DeployPlan {
    resolver::plan_with_mode(deploy, mods, winners, plan_mode)
}

/// A plan built from a disk scan instead of the library index. Tests that check deployment on real files use it.
#[cfg(test)]
pub fn plan_from_disk(
    deploy: &Deploy,
    library: &Path,
    profile: &[crate::domain::profiles::entry::ProfileMod],
) -> Result<DeployPlan, DeployError> {
    let mut mods = Vec::with_capacity(profile.len());

    for entry in profile {
        let files = if entry.enabled {
            scan::list_files(&library.join(&entry.folder))?
        } else {
            Vec::new()
        };
        mods.push(ModInput {
            id: entry.folder.clone().into(),
            folder: entry.folder.clone().into(),
            slot: entry.slot.clone().into(),
            display_name: entry.folder.clone().into(),
            enabled: entry.enabled,
            priority: entry.priority,
            files: files.into(),
            layout: None,
        });
    }

    Ok(plan(deploy, &mods, &BTreeMap::new()))
}
