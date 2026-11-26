//! What a restore would change, computed from the snapshot and the current stores.

use std::collections::BTreeSet;
use std::fs;

use sha2::Sha256;

use super::apply::resolved_profile;
use super::files::targets;
use crate::error::{AppError, io_at};
use crate::games::GameDef;
use crate::infra::disk;
use crate::infra::fs::list_files;
use crate::infra::io::digest_file;
use crate::snapshots::manifest::{clamp_u32, files_bytes, fits, load, manifest_bytes};
use crate::snapshots::view::RestorePreview;
use crate::snapshots::{Data, Settings, game_running_at, relative_path, scope};

/// What a restore would change, without writing anything. `running` tells the window to keep the action disabled.
pub fn preview(
    app: &Data,
    game: &GameDef,
    settings: &Settings,
    id: &str,
) -> Result<RestorePreview, AppError> {
    let scope = scope(app, &game.id)?;
    let manifest = load(&scope, id)?;
    let resolved = resolved_profile(&scope, &manifest)?;
    let targets = targets(&scope, game, settings, &manifest, &resolved.profile)?;
    let mut preview = RestorePreview {
        added: 0,
        replaced: 0,
        removed: 0,
        bytes: 0,
        free_kb: u32::MAX,
        enough_space: true,
        running: game_running_at(&game.id, &settings.game_root),
        mods: manifest.profile_state.as_ref().map_or(0, |state| {
            u32::try_from(state.list.mods.len()).unwrap_or(u32::MAX)
        }),
        recreates_profile: resolved.created,
    };
    let mut bytes = 0u64;
    let mut free_kb = u64::MAX;

    for (set, target) in &targets {
        let current: BTreeSet<String> = if target.is_dir() {
            list_files(target)
                .map_err(io_at(target))?
                .into_iter()
                .collect()
        } else {
            BTreeSet::new()
        };
        let mut saved = BTreeSet::new();

        for file in &set.files {
            saved.insert(file.path.clone());
            let path = target.join(relative_path(&file.path)?);

            match fs::metadata(&path) {
                Ok(meta) if meta.is_file() && meta.len() == file.size => {
                    let digest = digest_file::<Sha256>(&path, &mut |_| {}).map_err(io_at(&path))?;

                    if digest != file.sha256 {
                        preview.replaced += 1;
                        bytes = bytes.saturating_add(file.size);
                    }
                }
                Ok(meta) if meta.is_file() => {
                    preview.replaced += 1;
                    bytes = bytes.saturating_add(file.size);
                }
                _ => {
                    preview.added += 1;
                    bytes = bytes.saturating_add(file.size);
                }
            }
        }

        preview.removed += u32::try_from(current.difference(&saved).count()).unwrap_or(u32::MAX);

        if let Some(space) = disk::space_for(target) {
            free_kb = free_kb.min(space.free_kb);

            if !fits(space.free_kb, files_bytes(&set.files)) {
                preview.bytes = clamp_u32(bytes);
                preview.free_kb = clamp_u32(free_kb);
                preview.enough_space = false;
                return Ok(preview);
            }
        }
    }
    // The fresh backup is written under the app's data folder when the profile still exists, so that disk needs the
    // snapshot size as well. A recreated profile has nothing to back up.
    if !resolved.created
        && let Some(space) = disk::space_for(&scope.app.dir)
    {
        free_kb = free_kb.min(space.free_kb);

        if !fits(space.free_kb, manifest_bytes(&manifest)) {
            preview.bytes = clamp_u32(bytes);
            preview.free_kb = clamp_u32(free_kb);
            preview.enough_space = false;
            return Ok(preview);
        }
    }

    preview.bytes = clamp_u32(bytes);
    preview.free_kb = clamp_u32(free_kb);
    Ok(preview)
}
