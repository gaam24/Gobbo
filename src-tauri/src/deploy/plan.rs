//! A deployment plan built from a disk scan instead of the library index, for the tests that deploy real files.

use std::collections::BTreeMap;
use std::path::Path;

use super::DeployError;
use super::scan;
use crate::domain::resolver::{self, DeployPlan, ModInput, PlanMode};
use crate::games::Deploy;

/// A plan built from a disk scan instead of the library index. Tests that check deployment on real files use it.
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

    Ok(resolver::plan_with_mode(
        deploy,
        &mods,
        &BTreeMap::new(),
        PlanMode::Full,
    ))
}
