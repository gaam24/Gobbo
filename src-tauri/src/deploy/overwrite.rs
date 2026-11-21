//! The overwrite layer of an overlay: what the game and its tools wrote while they ran, kept out of the game folder.
//! The user can see those files, turn them into a mod of their own (a merged script, a generated patch, a config
//! the game wrote), or clear them.

use crate::context::Data;
use crate::context::load_library;
use crate::context::load_profiles;
use crate::context::load_stopped;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::context::running::game_running;
use crate::context::size_kb;
use crate::deploy::overlay::Session;
use crate::deploy::target::deployment_root;
use crate::domain::profiles::Entry as ProfileEntry;
use crate::error::AppError;
use crate::error::game_running as game_is_running_error;
use crate::error::io_at;
use crate::games;
use crate::library;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// One file in the overwrite layer, with its path in the game.
#[derive(Debug, Serialize, specta::Type)]
pub struct OverwriteFile {
    pub path: String,
    pub size_kb: u32,
}

#[derive(Debug, Serialize, specta::Type)]
pub struct OverwriteView {
    /// The folder of the layer, for opening it in the file manager.
    pub folder: String,
    pub files: Vec<OverwriteFile>,
}

/// The overwrite layer of one profile. It stays when the deployment is undone, until the user clears it.
fn overwrite_dir_for(data: &Data, profile: &str) -> PathBuf {
    Session::at(&data.profile_state(profile), Path::new("")).overwrite
}

/// The overwrite layer of the active profile.
fn overwrite_dir(data: &Data) -> PathBuf {
    overwrite_dir_for(data, &data.active_profile_id())
}

/// Whether one profile's overwrite layer holds anything. Cheap: it reads one folder entry. A caller that already has
/// the profiles (the status) passes the active id, so `profiles.json` is not read a second time.
pub fn has_overwrite_for(data: &Data, profile: &str) -> bool {
    fs::read_dir(overwrite_dir_for(data, profile)).is_ok_and(|mut entries| entries.next().is_some())
}

pub fn overwrite_files(root: &Data, game_id: &str) -> Result<OverwriteView, AppError> {
    games::load(game_id)?;
    let data = root.game(game_id);
    let folder = overwrite_dir(&data);
    let files = if folder.is_dir() {
        crate::infra::fs::list_files(&folder)
            .map_err(io_at(&folder))?
            .into_iter()
            .map(|path| OverwriteFile {
                size_kb: size_kb(fs::metadata(folder.join(&path)).map_or(0, |meta| meta.len())),
                path,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(OverwriteView {
        folder: folder.display().to_string(),
        files,
    })
}

/// Moves the files of the overwrite layer into a new mod named `name`, enabled at the top of the active list, so its
/// files win. The files keep their game paths. With a deployment root inside the game (`{game}/Data`), only the files
/// under that root move, relative to it.
pub fn overwrite_to_mod(root: &Data, game_id: &str, name: &str) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    let _index = crate::library::lock_index();
    let _profiles = lock_profiles();
    let (game, data, settings) = load_stopped(root, game_id)?;
    let name = name.trim();

    if name.is_empty() {
        return Err(AppError::user("overwrite.name_empty"));
    }

    let inner = deployment_root(&data, &game, &settings)
        .ok()
        .and_then(|root| {
            root.strip_prefix(settings.game_root.trim())
                .ok()
                .map(Path::to_path_buf)
        })
        .unwrap_or_default();
    let source = overwrite_dir(&data).join(&inner);
    let files = if source.is_dir() {
        crate::infra::fs::list_files(&source).map_err(io_at(&source))?
    } else {
        Vec::new()
    };

    if files.is_empty() {
        return Err(AppError::user("overwrite.empty"));
    }

    let library_dir = data.library();
    let folder = free_library_folder(&library_dir, name);
    let target = library_dir.join(&folder);

    for file in &files {
        let to = target.join(file);

        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(io_at(parent))?;
        }

        let from = source.join(file);
        fs::rename(&from, &to).map_err(io_at(&from))?;
    }

    remove_empty_dirs(&overwrite_dir(&data));

    let mut library = load_library(&data, &game)?;
    let mut version = library::scan_mod(&library_dir, &folder, &game)?;
    // The files have game paths, so the mod has no slot of its own.
    version.slot = String::new();
    version.info.name = name.to_string();
    version.family = library::family_id_from(None, name);
    let family = library::Family::of(&version);
    library.families.entry(family.id.clone()).or_insert(family);
    let id = version.family.clone();
    library.versions.insert(folder.clone(), version);
    data.save_library(library)?;

    let mut profiles = load_profiles(&data)?;
    let active = profiles.active_mut();
    active.remove_mods(|entry| entry.id == id);
    active.mods.insert(
        0,
        ProfileEntry {
            id,
            folder,
            enabled: true,
            priority: None,
        },
    );
    save_profiles(&data, &profiles)
}

/// Removes everything the game and its tools wrote into the active profile's overwrite layer.
pub fn clear_overwrite(root: &Data, game_id: &str) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    games::load(game_id)?;
    let data = root.game(game_id);

    if game_running(root, game_id)? {
        return Err(game_is_running_error());
    }

    let folder = overwrite_dir(&data);
    crate::infra::fs::remove_dir_if_exists(&folder).map_err(io_at(&folder))
}

/// A library folder name for `name` that is not taken yet: the name, then the name with a number.
fn free_library_folder(library: &Path, name: &str) -> String {
    let base = crate::domain::naming::safe_limited_file_name(name);
    crate::domain::naming::first_free(
        |number| crate::domain::naming::numbered_name(&base, number),
        |folder| !library.join(folder).exists(),
    )
}

/// Removes the empty folders under `dir`, deepest first. The folder itself stays.
fn remove_empty_dirs(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();

        if path.is_dir() {
            remove_empty_dirs(&path);
            let _ = fs::remove_dir(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn a_taken_library_folder_gets_the_next_free_number() {
        let library = scratch!("free-folder");
        assert_eq!(free_library_folder(&library, "Mod"), "Mod");

        fs::create_dir_all(library.join("Mod")).unwrap();
        assert_eq!(free_library_folder(&library, "Mod"), "Mod (2)");

        fs::create_dir_all(library.join("Mod (2)")).unwrap();
        assert_eq!(free_library_folder(&library, "Mod"), "Mod (3)");
    }

    #[test]
    fn a_second_thread_sees_the_held_state_lock_before_files_move() {
        let root = Data::new(scratch!("overwrite-lock"));
        let _held = crate::context::lock_state(&root.dir).unwrap();

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let error = overwrite_to_mod(&root, "no-game", "Merged").unwrap_err();
                sender.send(error.code).unwrap();
            });
        });
        assert_eq!(receiver.recv().unwrap(), "state.busy");
    }

    #[test]
    fn a_second_thread_sees_the_held_state_lock_before_the_overwrite_is_cleared() {
        let root = Data::new(scratch!("overwrite-clear-lock-busy"));
        let _held = crate::context::lock_state(&root.dir).unwrap();

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let error = clear_overwrite(&root, "no-game").unwrap_err();
                sender.send(error.code).unwrap();
            });
        });
        assert_eq!(receiver.recv().unwrap(), "state.busy");
    }

    #[test]
    fn clearing_the_overwrite_reenters_a_state_lock_its_caller_holds() {
        crate::test_support::games::install();
        let root = Data::new(scratch!("overwrite-clear-lock-reentrant"));
        let folder = overwrite_dir(&root.game(crate::test_support::games::SLOT_ID));
        fs::create_dir_all(folder.join("Mods")).unwrap();
        fs::write(folder.join("Mods/merged.ws"), "merged").unwrap();
        let held = crate::context::lock_state(&root.dir).unwrap();

        clear_overwrite(&root, crate::test_support::games::SLOT_ID).unwrap();
        assert!(!folder.exists());

        // The nested lock of the clear must not release the outer one.
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                sender
                    .send(crate::context::lock_state(&root.dir).is_err())
                    .unwrap();
            });
        });
        assert!(receiver.recv().unwrap(), "the outer lock is still held");
        drop(held);
    }
}
