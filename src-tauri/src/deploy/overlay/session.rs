//! The folders of an overlay deployment and the steps that build, reuse and undo them. The last part waits until the
//! platform has released a game folder that the overlay still covers.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use super::in_use;
#[cfg(not(windows))]
use super::linux::check;
use super::place::place;
use super::report::{layer_prefix, write_layer_dlls};
#[cfg(windows)]
use super::windows::check;
use crate::deploy::apply::{layer_jobs, write_settings};
use crate::deploy::journal::{Entry, Journal, JournalRoots};
use crate::deploy::{
    ApplyReport, DeployError, DeployProgress, DeployStage, Target, scan, state_is_overlay, undo,
};
use crate::domain::resolver::DeployPlan;
use crate::domain::user_data::Set as UserDataSet;
use crate::error::AppError;
use crate::error::FileError;
use crate::error::Message;
use crate::error::game_running as game_is_running_error;
use gobbo_overlay_core::Layers;
#[cfg(windows)]
use gobbo_overlay_core::{MountConfig, MountSpec};

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
            self.root.clone(),
            Some(self.mods.clone()),
            self.overwrite.clone(),
        )
    }

    /// The isolated redirects that add a mount outside the game folder.
    #[cfg(windows)]
    pub fn redirects(&self) -> impl Iterator<Item = &UserDataSet> {
        self.user_data.iter().filter(|set| set.redirects())
    }
}

/// The JSON file the hook reads instead of the three single-root environment variables. It is written before every
/// launch, so a profile switch never leaves a stale mount set behind.
#[cfg(windows)]
pub fn mounts_path(state: &Path) -> PathBuf {
    state.join("overlay-mounts.json")
}

/// The mount set of this session: the game folder plus every isolated redirect. The game is always the first mount.
#[cfg(windows)]
pub fn mount_config(session: &Session) -> MountConfig {
    let mut mounts = vec![MountSpec {
        id: MountSpec::GAME_ID.to_string(),
        guest: session.root.clone(),
        root: session.root.clone(),
        staging: Some(session.mods.clone()),
        upper: session.overwrite.clone(),
    }];
    mounts.extend(session.redirects().map(|set| MountSpec {
        id: set.id.clone(),
        guest: set.real.clone(),
        root: set.base.clone(),
        staging: None,
        upper: set.upper.clone(),
    }));
    MountConfig {
        version: MountConfig::VERSION,
        mounts,
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
    let session = Session::with_roots(
        &target.state,
        &target.root,
        &target.game_root,
        target.user_data.clone(),
    );
    // Each platform checks what its mount or its hook needs, before the first write.
    check(&session)?;
    // The layer mirrors the deployment root. A root inside the game folder (`{game}/Data`) is a folder inside the
    // layer; a root outside it (`{appdata}/Game`) is the layer itself.
    let inner = target.root_in_game().unwrap_or_default();
    let base = session.mods.join(&inner);

    // The layer from the previous session of this profile stays when it matches the plan. Otherwise we build it again.
    // The old files are just hardlinks, so removing them does not touch the library or the game.
    let reused = matches_plan(plan, library, &session.mods, &inner)?;
    // The layer is removed under a game that still runs from it, so a rebuild waits until the overlay is free.
    if !reused && in_use(&session.root) {
        return Err(DeployError::OverlayInUse { path: session.root });
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
    journal.append(&Entry::LayerPrepared {
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

    let mut known_dirs = rustc_hash::FxHashSet::default();
    crate::deploy::progress::write_jobs(
        &jobs,
        progress,
        |job| {
            if known_dirs.insert(job.dir.clone()) {
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
    write_settings(journal, target, &plan.settings)?;
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
    has_load_order: bool,
    running: bool,
    roots: &JournalRoots,
) -> Result<(), AppError> {
    for (profile, path) in prepared {
        if profile.as_str() == active {
            continue;
        }
        // An overlay layer can stay for fast switching, unless the game's order file is shared between profiles.
        if !state_is_overlay(path) || has_load_order {
            if running {
                return Err(game_is_running_error());
            }

            undo(&roots.at_state(path.clone())).map_err(AppError::from)?;
        }
    }

    Ok(())
}

/// Whether the layer from the previous session matches the plan: the same set of files, and each one is a hardlink to a file
/// in the library. Then nothing has to be built again, which saves the most time with large modpacks.
pub(in crate::deploy) fn matches_plan(
    plan: &DeployPlan,
    library: &Path,
    mods: &Path,
    inner: &Path,
) -> Result<bool, DeployError> {
    if !mods.is_dir() {
        return Ok(false);
    }
    // The scan lists the layer's files with `/` and sorted, so the plan's paths take the same form.
    let prefix = layer_prefix(inner);
    let mut planned = Vec::with_capacity(plan.files.len());

    for file in &plan.files {
        let src = library.join(plan.folder(file)).join(plan.rel_path(file));
        let layered = format!("{prefix}{}", file.dest);

        if !same_file(&src, &mods.join(&layered)) {
            return Ok(false);
        }

        planned.push(layered);
    }

    let present = scan::list_files(mods)?;
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

/// Waits while an overlay of the game folder `root` is still used by a process that outlived its launcher. The game counts
/// as running until its last process has ended. Returns at once when no overlay is mounted there.
pub fn wait_released(root: &Path) {
    while in_use(root) {
        thread::sleep(RELEASE_POLL);
    }
}
