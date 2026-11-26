//! The folders of an overlay deployment and the steps that build, reuse and undo them. The last part waits until the
//! platform has released a game folder that the overlay still covers.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use super::in_use;
#[cfg(not(windows))]
use super::linux::check;
use super::place::place;
use super::report::{layer_prefix, write_layer_dlls};
#[cfg(windows)]
use super::windows::check;
use crate::deploy::apply::layer_jobs;
use crate::deploy::error::DeployError;
use crate::deploy::journal::{Journal, JournalOp, JournalRoots};
use crate::deploy::model::{DeployProgress, DeployStage};
use crate::deploy::order;
use crate::deploy::undo::undo;
use crate::deploy::{ApplyReport, Target, scan, state_is_overlay};
use crate::domain::pathkey::path_key_into;
use crate::domain::resolver::DeployPlan;
use crate::domain::user_data::Set as UserDataSet;
use crate::error::{AppError, FileError, Message, game_running as game_is_running_error};
use gobbo_overlay_core::Layers;

/// The folders of one overlay session, directly under the profile's state folder. `overwrite` and `.work`
/// stay between sessions, because `overwrite` holds the game's writes.
#[derive(Debug)]
pub struct Session {
    /// The profile's state folder, where the journal is.
    pub state: PathBuf,
    /// The mod layer, on top: paths relative to the game.
    pub mods: PathBuf,
    /// The game's writes from this and earlier sessions.
    pub overwrite: PathBuf,
    /// The overlayfs work folder. It must be on the same filesystem as `overwrite`.
    pub work: PathBuf,
    /// The deployment root that we mount the view on. It is the game folder for most games, or a folder outside it
    /// (for example `%APPDATA%`) for a game that keeps its mods there.
    pub root: PathBuf,
    /// The game folder, where the game program starts and where a game-root mod loader lives.
    pub game_root: PathBuf,
    /// The user-data sets of the active profile. Only the Windows hook turns a redirect set into an extra mount
    /// outside the game folder; the other platforms copy the folder instead.
    #[cfg(windows)]
    pub user_data: Vec<UserDataSet>,
}

impl Session {
    /// A session for a deployment root that is also the game folder, with no user-data redirects.
    pub fn at(state: &Path, root: &Path) -> Self {
        Self::with_roots(state, root, root, Vec::new())
    }

    /// A session with the deployment root, the game folder and the profile's user-data sets. `prepare` builds it from
    /// the target so the Windows hook sees the same redirects the deployment planned. The other platforms copy a
    /// redirect and do not keep the sets. The root and the game folder differ for a game that keeps its mods outside
    /// the game folder.
    pub fn with_roots(
        state: &Path,
        root: &Path,
        game_root: &Path,
        user_data: Vec<UserDataSet>,
    ) -> Self {
        #[cfg(not(windows))]
        drop(user_data);
        Self {
            state: state.to_path_buf(),
            mods: state.join("mods"),
            overwrite: state.join("overwrite"),
            work: state.join(".work"),
            root: root.to_path_buf(),
            game_root: game_root.to_path_buf(),
            #[cfg(windows)]
            user_data,
        }
    }

    /// The layers of this session, as the platform sees them.
    pub fn layers(&self) -> Layers {
        Layers::new(
            self.view_root().to_path_buf(),
            Some(self.mods.clone()),
            self.overwrite.clone(),
        )
    }

    /// The folder the view is mounted on. `prepare` builds the mod layer as a mirror of the game folder when the
    /// deployment root is the game folder or a folder inside it, so the view covers the game folder then; a root outside
    /// the game folder is mounted on its own.
    pub fn view_root(&self) -> &Path {
        if crate::deploy::root_in_game(&self.root, &self.game_root).is_some() {
            &self.game_root
        } else {
            &self.root
        }
    }

    /// The layers as the game sees them from its own folder, where its programs and their proxy DLLs are. A root
    /// outside the game folder holds none of the game's programs: `None`.
    pub fn game_layers(&self) -> Option<Layers> {
        crate::deploy::root_in_game(&self.root, &self.game_root).map(|_| self.layers())
    }

    /// The isolated redirects that add a mount outside the game folder.
    #[cfg(windows)]
    pub fn redirects(&self) -> impl Iterator<Item = &UserDataSet> {
        self.user_data.iter().filter(|set| set.is_redirected())
    }
}

/// The platform half of the support check, shared by preflight and prepare.
const fn supported() -> Result<(), DeployError> {
    if !crate::platform::supports_overlay() {
        return Err(DeployError::OverlayUnsupported);
    }

    Ok(())
}

/// Checks whether overlay can run on this computer and with these paths, without writing anything. The preflight uses
/// it on the game and the active profile's state folder; `prepare` repeats the same check before its first write.
pub fn check_support(state: &Path, root: &Path) -> Result<(), DeployError> {
    supported()?;
    check(&Session::at(state, root))
}

/// Prepares the layer for the plan: hardlinks to the library in `mods/` and the settings with a journal entry. The
/// game folder does not change. Calls `progress` after each hardlink; a reused layer reports its existing file count
/// at once.
pub fn prepare_with_progress(
    plan: &DeployPlan,
    library: &Path,
    target: &Target,
    journal: &Journal,
    progress: &mut dyn FnMut(DeployProgress),
) -> Result<ApplyReport, DeployError> {
    supported()?;
    #[cfg(windows)]
    let user_data = target.user_data.clone();
    #[cfg(not(windows))]
    let user_data = Vec::new();
    let session = Session::with_roots(&target.state, &target.root, &target.game_root, user_data);
    // Each platform checks what its mount or its hook needs, before the first write.
    check(&session)?;
    // The layer mirrors the deployment root. A root inside the game folder (`{game}/Data`) is a folder inside the
    // layer; a root outside it (`{appdata}/Game`) is the layer itself.
    let inner = crate::deploy::root_in_game(&target.root, &target.game_root).unwrap_or_default();
    let base = session.mods.join(&inner);

    // The layer from the previous session of this profile stays when it matches the plan. Otherwise we build it again.
    // The old files are just hardlinks, so removing them does not touch the library or the game.
    let reused = matches_plan(
        plan,
        library,
        &session.mods,
        &inner,
        target.order_file.as_deref(),
    )?;
    // The layer is removed under a game that still runs from it, so a rebuild waits until the overlay is free.
    if !reused && in_use(session.view_root()) {
        return Err(DeployError::OverlayInUse {
            path: session.view_root().to_path_buf(),
        });
    }

    if !reused && session.mods.exists() {
        fs::remove_dir_all(&session.mods).map_err(|e| FileError::new(&session.mods, e))?;
    }
    // A whiteout may hide a file of a mod that the new layer no longer has, or the file of another mod with the same
    // path. So a rebuilt layer starts without the whiteouts of the old one, including its journal.
    let whiteouts = session.layers().whiteouts_file();
    let whiteouts_journal = session.layers().whiteouts_journal_file();

    if !reused && whiteouts.exists() {
        fs::remove_file(&whiteouts).map_err(|e| FileError::new(&whiteouts, e))?;
    }

    if !reused && whiteouts_journal.exists() {
        fs::remove_file(&whiteouts_journal).map_err(|e| FileError::new(&whiteouts_journal, e))?;
    }

    let jobs = if reused {
        Vec::new()
    } else {
        layer_jobs(plan, library, &base)?
    };

    // The entry goes in before the first write, so an interrupted session can be undone.
    journal.append(&JournalOp::LayerPrepared {
        mods: session.mods.clone(),
        overwrite: session.overwrite.clone(),
    })?;

    for dir in [&session.mods, &session.overwrite, &session.work] {
        fs::create_dir_all(dir).map_err(|e| FileError::new(dir, e))?;
    }

    let mut report = ApplyReport::default();

    if reused {
        report.files = plan.files.len();
        progress(DeployProgress::at(
            DeployStage::Writing,
            report.files,
            report.files,
        ));
    }

    let mut known_dirs: rustc_hash::FxHashSet<PathBuf> = rustc_hash::FxHashSet::default();
    crate::deploy::progress::write_jobs(
        &jobs,
        progress,
        |job| {
            if !known_dirs.contains(&job.dir) {
                known_dirs.insert(job.dir.clone());
                fs::create_dir_all(&job.dir).map_err(|e| FileError::new(&job.dir, e))?;
            }

            Ok(())
        },
        |job| {
            place(&job.src, &job.dst)?;
            report.files += 1;
            Ok(())
        },
    )?;

    progress(DeployProgress::at(
        DeployStage::Settings,
        report.files,
        report.files,
    ));
    order::write(journal, target, plan, library, &base)?;
    write_layer_dlls(&session, plan, &inner);
    Ok(report)
}

/// Undoes the overlay session. The mod layer stays in `mods/`, so the next deployment of this profile does not build
/// it again. `overwrite/` stays too, because it holds the game's writes, so we warn when it is not empty.
pub fn finish(overwrite: &Path, warnings: &mut Vec<Message>) {
    let has_writes = fs::read_dir(overwrite).is_ok_and(|mut entries| entries.next().is_some());

    if has_writes {
        warnings.push(Message::new("undo.overlay_writes_kept").with("path", overwrite.display()));
    }
}

/// Makes room for an overlay deployment. The other profiles' deployments in the game folder are undone first, because
/// their files would leak into the base layer. Their overlay layers stay for fast switching. While the game runs, a
/// deployment that must be undone is refused. A `mods.settings` that the game shares between profiles lets only one
/// overlay profile own it, so the other layers make way too.
pub fn make_room(
    prepared: &BTreeMap<String, PathBuf>,
    active: &str,
    has_settings_file: bool,
    running: bool,
    roots: &JournalRoots,
) -> Result<(), AppError> {
    for (profile, path) in prepared {
        if profile.as_str() == active {
            continue;
        }
        // An overlay layer can stay for fast switching, unless the game's order file is shared between profiles.
        if !state_is_overlay(path) || has_settings_file {
            if running {
                return Err(game_is_running_error());
            }

            undo(&roots.at_state(path.clone())).map_err(AppError::from)?;
        }
    }

    Ok(())
}

/// Whether the layer from the previous session matches the plan: the same set of files, and each one is a hardlink to a file
/// in the library. Then nothing has to be built again, which saves the most time with large modpacks. `order_file` is the
/// list `write_order` writes into the layer. When the plan has rows for it, that file is written again on every apply, so
/// it is not a hardlink and the check leaves it out.
pub(in crate::deploy) fn matches_plan(
    plan: &DeployPlan,
    library: &Path,
    mods: &Path,
    inner: &Path,
    order_file: Option<&str>,
) -> Result<bool, DeployError> {
    if !mods.is_dir() {
        return Ok(false);
    }
    // The scan lists the layer's files with `/` and sorted, so the plan's paths take the same form.
    let prefix = layer_prefix(inner);
    let mut rewritten_key = String::new();
    let rewritten = order_file.filter(|_| !plan.settings.is_empty()).map(|rel| {
        let path = format!("{prefix}{rel}");
        path_key_into(&path, &mut rewritten_key)
    });
    let mut layered_key = String::new();
    let mut is_rewritten = |layered: &str| {
        rewritten.as_ref().is_some_and(|range| {
            layered_key.clear();
            path_key_into(layered, &mut layered_key);
            rewritten_key[range.start..range.end] == layered_key[..]
        })
    };
    let mut planned = Vec::with_capacity(plan.files.len());

    for file in &plan.files {
        let layered = format!("{prefix}{}", file.dest);

        if is_rewritten(&layered) {
            continue;
        }

        let src = library.join(plan.folder(file)).join(plan.rel_path(file));

        if !same_file(&src, &mods.join(&layered)) {
            return Ok(false);
        }

        planned.push(layered);
    }

    let mut present = scan::list_files(mods)?;
    present.retain(|layered| !is_rewritten(layered));
    planned.sort_unstable();
    Ok(planned == present)
}

/// The same file on disk, that is, a hardlink (the same inode, or on Windows the same volume and file index).
/// The `same-file` crate reads that identity on every platform, because the standard library does not offer the
/// Windows one on stable Rust.
fn same_file(src: &Path, dst: &Path) -> bool {
    ::same_file::is_same_file(src, dst).unwrap_or(false)
}

/// How often `wait_released` looks at the overlay again.
const RELEASE_POLL: Duration = Duration::from_secs(1);
/// The longest a post-exit wait holds the running marker: a process that outlived its launcher must not leave the app
/// showing a game as running forever. Quit budgets this together with the post-exit save's lock wait.
const RELEASE_TIMEOUT: Duration = Duration::from_secs(20);

/// Waits while an overlay of the game folder `root` is still used by a process that outlived its launcher. The game counts
/// as running until its last process has ended. Returns at once when no overlay is mounted there.
pub fn wait_released(root: &Path) {
    wait_released_with(root, RELEASE_TIMEOUT, in_use);
}

/// `wait_released` with an explicit budget and use check. On a timeout it logs a warning and returns, so the caller
/// clears its running marker even when an overlay process never went away.
pub fn wait_released_with(root: &Path, timeout: Duration, in_use: impl Fn(&Path) -> bool) {
    let started = Instant::now();
    while in_use(root) {
        if started.elapsed() >= timeout {
            crate::infra::log::warn(format!(
                "overlay: {} is still in use after {} s; continuing",
                root.display(),
                timeout.as_secs()
            ));
            return;
        }
        thread::sleep(RELEASE_POLL);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn a_zero_timeout_returns_while_the_overlay_is_still_in_use() {
        let calls = Cell::new(0);
        wait_released_with(Path::new("unused-game-folder"), Duration::ZERO, |_| {
            calls.set(calls.get() + 1);
            true
        });
        assert_eq!(calls.get(), 1);
    }
}
