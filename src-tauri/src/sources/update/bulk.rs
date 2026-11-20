//! "Update all": one pass over the mods whose saved check found a newer version.

use std::sync::atomic::AtomicBool;

use crate::context::Data;
use crate::error::{AppError, Message};
use crate::infra::tasks::cancelled;

use super::{
    UpdateAllProgress, UpdateAllStage, UpdateAllView, UpdateChange, UpdateProgress, UpdateTarget,
    update_mod_cancellable,
};

/// Updates every mod whose saved check found another version, or only the mods `only` names when it is present. A mod
/// whose newest version is installed already is only selected. One failure does not stop the rest: the view keeps each
/// error and the fresh status shows what changed.
pub fn update_all(
    root: &Data,
    game_id: &str,
    only: Option<&[String]>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(UpdateAllProgress),
) -> Result<UpdateAllView, AppError> {
    let status = crate::status::status(root, game_id)?;
    let targets: Vec<UpdateTarget> = status
        .mods
        .iter()
        .filter_map(|mod_view| {
            let update = mod_view.update.as_ref()?;
            // The same rule the update button reads (`UpdateReport::has_update`).
            if !update.has_update {
                return None;
            }

            if !wanted(only, &mod_view.id) {
                return None;
            }

            Some(UpdateTarget {
                id: mod_view.id.clone(),
                name: mod_view.info.name.clone(),
                provider: update.provider.clone(),
                installed_folder: update.installed_folder.clone(),
                previous: mod_view.folder.clone(),
            })
        })
        .collect();
    let total = u32::try_from(targets.len()).unwrap_or(u32::MAX);
    let mut updated = 0;
    let mut failed = Vec::new();
    let mut changes = Vec::new();
    let mut stopped = false;

    for (index, target) in targets.iter().enumerate() {
        if cancelled(cancel) {
            stopped = true;
            break;
        }

        let done = u32::try_from(index).unwrap_or(u32::MAX);
        let step = |stage: UpdateAllStage, part: u32| UpdateAllProgress {
            name: target.name.clone(),
            done,
            total,
            stage,
            percent: overall_percent(done, total, part),
        };
        let outcome = if let Some(folder) = target.installed_folder.as_deref() {
            progress(step(UpdateAllStage::Switching, 100));
            crate::profiles::actions::set_mod_version(root, game_id, &target.id, folder)
        } else {
            progress(step(UpdateAllStage::Downloading, 0));
            let mut send = |update: UpdateProgress| {
                progress(step(UpdateAllStage::from(update.stage), update.percent));
            };
            update_mod_cancellable(
                root,
                game_id,
                &target.id,
                &target.provider,
                cancel,
                &mut send,
            )
        };

        match outcome {
            Ok(()) => {
                updated += 1;
                changes.push(UpdateChange {
                    id: target.id.clone(),
                    name: target.name.clone(),
                    previous: target.previous.clone(),
                });
            }
            Err(error) if cancelled(cancel) && error.is_cancelled() => {
                stopped = true;
                break;
            }
            Err(error) => failed.push(Message::from(error)),
        }
    }

    Ok(UpdateAllView {
        status: crate::status::status(root, game_id)?,
        updated,
        failed,
        changes,
        cancelled: stopped,
    })
}

/// Whether a target belongs to the run. `None` means the whole list.
fn wanted(only: Option<&[String]>, id: &str) -> bool {
    only.is_none_or(|ids| ids.iter().any(|wanted| wanted == id))
}

/// The percentage of the whole run: the finished mods plus the part of the current one.
fn overall_percent(done: u32, total: u32, part: u32) -> u32 {
    if total == 0 {
        return 100;
    }

    done.saturating_mul(100)
        .saturating_add(part)
        .checked_div(total)
        .unwrap_or(100)
        .min(100)
}

#[cfg(test)]
mod tests {
    use super::{overall_percent, wanted};

    #[test]
    fn the_overall_percentage_counts_the_finished_mods_and_the_current_part() {
        assert_eq!(overall_percent(0, 3, 0), 0);
        assert_eq!(overall_percent(0, 3, 100), 33);
        assert_eq!(overall_percent(1, 3, 50), 50);
        assert_eq!(overall_percent(2, 3, 100), 100);
        assert_eq!(overall_percent(0, 0, 0), 100);
    }

    #[test]
    fn a_run_of_chosen_mods_skips_the_others() {
        let chosen = ["nexus:1".to_string(), "nexus:2".to_string()];
        assert!(wanted(None, "nexus:1"));
        assert!(wanted(Some(&chosen), "nexus:2"));
        assert!(!wanted(Some(&chosen), "nexus:3"));
        assert!(!wanted(Some(&[]), "nexus:1"));
    }
}
