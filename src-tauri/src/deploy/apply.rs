//! Applying a deployment plan: the journal, folder creation, file placement and the settings write.

use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};

use rustc_hash::FxHasher;

use crate::domain::deployment::DeploymentRecord;
use crate::domain::resolver::{DeployPlan, SettingsRow};
use crate::error::FileError;

use super::journal::{Entry, Journal};
use super::overlay;
use super::settings;
use super::undo;
use super::{ApplyReport, DeployError, DeployProgress, Target};
use crate::context::data::JOURNAL_FILE;

/// Deploys the plan into the overlay layer. Refuses when something is already deployed or the plan has problems.
/// All checks happen before the first write. It does not touch the game folder; it only prepares the layer.
/// The no-progress form the deployment tests use. The app always passes the channel.
#[cfg(any(test, feature = "bench"))]
pub fn apply(
    plan: &DeployPlan,
    library: &Path,
    target: &Target,
) -> Result<ApplyReport, DeployError> {
    apply_with_progress(plan, library, target, &mut |_| {})
}

/// Applies the plan with a progress callback. The callback runs for each placed file and once for the settings write;
/// the caller turns those steps into the window's progress channel.
pub fn apply_with_progress(
    plan: &DeployPlan,
    library: &Path,
    target: &Target,
    progress: &mut dyn FnMut(DeployProgress),
) -> Result<ApplyReport, DeployError> {
    let journal = Journal::new(target.state.join(JOURNAL_FILE));

    if journal.exists() {
        if journal.is_complete() {
            return Err(DeployError::AlreadyApplied);
        }
        // An apply stopped before its completion marker. Undo exactly what its journal recorded, then start clean;
        // a partial deployment must never be mistaken for an applied one.
        match undo::undo(&target.journal_roots()) {
            Ok(_) => {}
            // An empty or fully unreadable journal has nothing to revert, but it still blocks the state folder until
            // the next apply retires it.
            Err(DeployError::NothingToUndo) => journal.remove()?,
            Err(error) => return Err(error),
        }
    }

    if !plan.problems.is_empty() {
        return Err(DeployError::Blocked(plan.problems.clone()));
    }

    if let Some(settings) = &target.settings
        && !settings.parent().is_some_and(Path::is_dir)
    {
        return Err(FileError::new(settings, io::Error::from(io::ErrorKind::NotFound)).into());
    }

    fs::create_dir_all(&target.state).map_err(|e| FileError::new(&target.state, e))?;

    let report = overlay::prepare_with_progress(plan, library, target, &journal, progress)?;
    mark_complete(&journal, &report, plan)?;
    Ok(report)
}

/// Writes the completion marker only after the last file and the settings are in place. A status reads this marker,
/// not the journal, so an interrupted apply is never reported as applied.
fn mark_complete(
    journal: &Journal,
    report: &ApplyReport,
    plan: &DeployPlan,
) -> Result<(), DeployError> {
    journal.complete(&DeploymentRecord {
        schema_version: crate::domain::STATE_SCHEMA,
        files: u32::try_from(report.files).unwrap_or(u32::MAX),
        plan_hash: plan_fingerprint(plan),
    })
}

/// A stable fingerprint of the planned files and settings rows, so a future repair can compare the journal with the
/// list that produced it.
fn plan_fingerprint(plan: &DeployPlan) -> String {
    let mut hasher = FxHasher::default();

    for file in &plan.files {
        file.dest.hash(&mut hasher);
        file.source.hash(&mut hasher);
        file.file.hash(&mut hasher);
    }

    for row in &plan.settings {
        row.folder.hash(&mut hasher);
        row.enabled.hash(&mut hasher);
        row.priority.hash(&mut hasher);
    }

    format!("{:016x}", hasher.finish())
}

/// The file a deployment reads the settings from. A redirect writes its upper copy, but the first write must start from
/// the lower file the game already has: otherwise the sections Gobbo does not own would disappear from the game.
fn settings_source(target: &Target, path: &Path) -> PathBuf {
    if path.is_file() {
        return path.to_path_buf();
    }

    target
        .user_data
        .iter()
        .find_map(|set| {
            if !set.redirects() {
                return None;
            }

            let relative = path.strip_prefix(&set.upper).ok()?;
            let lower = set.base.join(relative);
            lower.is_file().then_some(lower)
        })
        .unwrap_or_else(|| path.to_path_buf())
}

/// Writes the mod settings. The journal gets the sections it changes, so `undo` puts back only those. The file is not
/// written when no row needs a section, so a missing settings file is not created empty. The journal entry is always
/// written: it is what marks the deployment as deployed.
pub fn write_settings(
    journal: &Journal,
    target: &Target,
    rows: &[SettingsRow],
) -> Result<(), DeployError> {
    let Some(path) = &target.settings else {
        return Ok(());
    };
    let source = settings_source(target, path);
    let before = settings::read(&source)?;
    let (text, touched) = settings::merge(before.as_deref(), rows);
    let changed = !touched.is_empty();
    journal.append(&Entry::SettingsSections {
        path: path.clone(),
        existed: before.is_some(),
        touched,
    })?;

    if !changed {
        return Ok(());
    }

    settings::write(path, &text)
}

pub(super) struct Job {
    pub(super) src: PathBuf,
    pub(super) dst: PathBuf,
    pub(super) dir: PathBuf,
}

/// The files of a layer that was just removed: the source must exist, and no target can. `base` is the overlay layer.
pub(super) fn layer_jobs(
    plan: &DeployPlan,
    library: &Path,
    base: &Path,
) -> Result<Vec<Job>, DeployError> {
    let mut jobs = Vec::with_capacity(plan.files.len());

    for file in &plan.files {
        let src = library.join(plan.folder(file)).join(plan.rel_path(file));

        if !src.is_file() {
            return Err(DeployError::MissingFile { path: src });
        }

        let dst = base.join(&file.dest);
        let dir = dst.parent().unwrap_or(base).to_path_buf();
        jobs.push(Job { src, dst, dir });
    }

    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::resolver::PlannedFile;

    #[test]
    fn the_plan_fingerprint_changes_with_files_and_settings() {
        let mut plan = DeployPlan::default();
        let empty = plan_fingerprint(&plan);

        plan.files.push(PlannedFile {
            source: 0,
            file: 0,
            dest: "Mods/modA/x.ws".into(),
            game_path: false,
        });
        let with_file = plan_fingerprint(&plan);
        assert_ne!(empty, with_file);

        plan.settings.push(SettingsRow {
            folder: "modA".into(),
            enabled: true,
            priority: 1,
        });
        let with_row = plan_fingerprint(&plan);
        assert_ne!(with_file, with_row);
        assert_eq!(with_row, plan_fingerprint(&plan));
    }
}
