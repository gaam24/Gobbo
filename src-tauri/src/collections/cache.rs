//! The local cache of a collection revision: the plan and the archive. It is keyed by the provider, the game and the
//! collection, so a profile can be resumed without the network and an update does not overwrite the old recipe.

use std::fs;
use std::path::{Path, PathBuf};

use crate::context::Data;
use crate::domain::collection::{CollectionInfo, CollectionOrigin, CollectionPlan, PLAN_FORMAT};
use crate::error::{AppError, io_at};

/// The folder of one revision, under `collections`. Every reader and writer builds the path the same way.
fn revision_dir(
    collections: &Path,
    provider: &str,
    game_key: &str,
    slug: &str,
    revision: u32,
) -> PathBuf {
    collections
        .join(crate::domain::naming::compact_component(provider))
        .join(crate::domain::naming::compact_component(game_key))
        .join(crate::domain::naming::compact_component(slug))
        .join(format!("r{revision}"))
}

/// The folder of one revision.
pub fn dir(data: &Data, info: &CollectionInfo) -> PathBuf {
    revision_dir(
        &data.collections(),
        &info.provider,
        &info.game_key,
        &info.slug,
        info.revision,
    )
}

/// The plan file of one revision.
fn plan_path(data: &Data, info: &CollectionInfo) -> PathBuf {
    revision_dir(
        &data.collections(),
        &info.provider,
        &info.game_key,
        &info.slug,
        info.revision,
    )
    .join("plan.json")
}

/// The archive of one revision, when the cache has it.
pub fn archive(data: &Data, info: &CollectionInfo) -> Option<PathBuf> {
    let dir = dir(data, info);
    let entries = fs::read_dir(&dir).ok()?;
    let mut candidates: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(crate::infra::archive::is_archive)
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

/// Reads the cached plan, or `None` when it is missing, damaged or from another format.
pub fn read_plan(data: &Data, info: &CollectionInfo) -> Option<CollectionPlan> {
    let plan: CollectionPlan = crate::infra::json::read_json_or_none(&plan_path(data, info))?;
    (plan.format == PLAN_FORMAT).then_some(plan)
}

/// Reads the cached plan of a profile origin, or the missing-plan error. A caller that wants the slug in the error
/// adds it to the result.
pub fn read_plan_or_missing(
    data: &Data,
    origin: &CollectionOrigin,
) -> Result<CollectionPlan, AppError> {
    let path = revision_dir(
        &data.collections(),
        &origin.provider,
        &origin.game_key,
        &origin.slug,
        origin.revision,
    )
    .join("plan.json");
    let plan: CollectionPlan = crate::infra::json::read_json_or_none(&path)
        .ok_or_else(|| AppError::user("collection.cache_missing"))?;
    (plan.format == PLAN_FORMAT)
        .then_some(plan)
        .ok_or_else(|| AppError::user("collection.cache_missing"))
}

/// Writes the plan atomically.
pub fn write_plan(data: &Data, plan: &CollectionPlan) -> Result<(), AppError> {
    let path = plan_path(data, &plan.info);
    crate::infra::json::write_json(&path, plan)
}

/// Makes the revision folder.
pub fn prepare_dir(data: &Data, info: &CollectionInfo) -> Result<PathBuf, AppError> {
    let dir = dir(data, info);
    fs::create_dir_all(&dir).map_err(io_at(&dir))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> CollectionInfo {
        CollectionInfo {
            revision_label: String::new(),
            provider: "nexus".into(),
            game_key: "cyberpunk2077".into(),
            slug: "n0nymh".into(),
            revision: 49,
            latest_revision: 49,
            name: "CET+Essentials".into(),
            author: None,
            summary: String::new(),
            description: String::new(),
            instructions: String::new(),
            picture_url: None,
            adult: false,
            total_size_kb: 0,
            mod_count: 0,
            game_versions: Vec::new(),
            changelog: None,
            url: String::new(),
        }
    }

    #[test]
    fn the_cache_path_is_built_from_safe_components() {
        let data = Data::new(crate::test_support::scratch!("collection-cache"));
        let path = dir(&data, &info());
        assert!(path.ends_with("nexus/cyberpunk2077/n0nymh/r49"));
        let mut unsafe_info = info();
        unsafe_info.slug = "../escape".into();
        let path = dir(&data, &unsafe_info);
        assert!(!path.to_string_lossy().contains(".."));
    }
}
