//! Writing provider update results into the library: sources, pages, requirements, sync and matched archives.

use std::collections::BTreeSet;

use crate::context::Data;
use crate::domain::source::{ProviderId, SourceMeta, SourceMod, SourceRef};
use crate::error::AppError;
use crate::games::GameDef;
use crate::infra::clock::now_seconds;
use crate::library::{Family, Library};
use crate::providers::SourceUpdate;

/// Stores what the providers returned and saves the index only when something changed. The requests take a while, so
/// the library is read again first: what changed meanwhile is kept.
pub(super) fn store_updates(
    data: &Data,
    game: &GameDef,
    updates: &[SourceUpdate],
) -> Result<std::sync::Arc<Library>, AppError> {
    let _index = crate::library::lock_index();
    let mut library = crate::context::load_library(data, game)?;
    let mut changed = false;
    let checked_at = now_seconds();
    for update in updates {
        if let Some((digest, file_id)) = &update.matched {
            changed |= set_archive_id(
                &mut library,
                digest,
                &update.source.provider,
                file_id,
                ArchiveId::File,
            );
        }
        if let Some((digest, part_id)) = &update.matched_part {
            changed |= set_archive_id(
                &mut library,
                digest,
                &update.source.provider,
                part_id,
                ArchiveId::Part,
            );
        }
        let Some(family) = library.families.get_mut(&update.family) else {
            continue;
        };
        if let Some((_, part_id)) = &update.matched_part
            && let Some(entry) = family.sources.get_mut(&update.source.provider)
            && entry.part_id.is_none()
        {
            entry.part_id = Some(part_id.clone());
            changed = true;
        }
        if let Some(read) = &update.meta
            && apply_meta(family, &update.source, read, checked_at)
        {
            changed = true;
        }
        if apply_latest(family, update) {
            changed = true;
        }
        if drop_stale_ignore(family, update) {
            changed = true;
        }
        if let Some(requirements) = &update.requirements {
            apply_requirements(
                family,
                &update.source.provider,
                requirements,
                &update.dlcs,
                update.sync.as_ref(),
            );
            changed = true;
        }
    }
    if changed {
        return data.save_library(library);
    }
    Ok(std::sync::Arc::new(library))
}

/// Which provider id list of an archive a check fills in.
enum ArchiveId {
    /// The exact file the provider lists.
    File,
    /// The persistent part the file belongs to.
    Part,
}

/// Stores one provider id with an archive. Answers whether the archive changed; an unknown digest is left alone.
fn set_archive_id(
    library: &mut Library,
    digest: &str,
    provider: &ProviderId,
    id: &str,
    which: ArchiveId,
) -> bool {
    let Some(archive) = library.archives.get_mut(digest) else {
        return false;
    };
    let ids = match which {
        ArchiveId::File => &mut archive.source_files,
        ArchiveId::Part => &mut archive.source_parts,
    };
    if ids.get(provider).map(String::as_str) == Some(id) {
        return false;
    }
    ids.insert(provider.clone(), id.to_string());
    true
}

/// Saves the page a provider returned for one source of a family. Reading the page is the check, so the page carries the
/// time of this run whatever the provider stamped; the list shows that time as "last checked". Used by the update check
/// and by a metadata refresh, so both write a page the same way.
pub fn apply_meta(
    family: &mut Family,
    source: &SourceRef,
    read: &SourceMeta,
    checked_at: u32,
) -> bool {
    let mut meta = read.clone();
    meta.checked_at = Some(checked_at);
    let entry = family
        .sources
        .entry(source.provider.clone())
        .or_insert_with(|| SourceMod {
            mod_id: source.mod_id.clone(),
            ..SourceMod::default()
        });
    let mut changed = entry.mod_id.is_empty();
    if changed {
        entry.mod_id.clone_from(&source.mod_id);
    }
    changed |= entry.fill_missing_part(
        source.part_id.as_deref(),
        source.part_name.as_deref(),
        source.part_category.as_deref(),
    );
    if entry.meta.as_ref() != Some(&meta) {
        entry.meta = Some(meta);
        changed = true;
    }
    changed
}

/// The newest version of a check, or `None` when the check failed or brought no version.
fn latest_of(update: &SourceUpdate) -> Option<&str> {
    if update.error.is_some() {
        return None;
    }
    update.latest.as_deref()
}

/// Saves the newest version and, when the provider named one, the newest file id of the check. A failed check keeps
/// the old values. A check that fell back to the page version drops a stale file id, so the status uses the string.
fn apply_latest(family: &mut Family, update: &SourceUpdate) -> bool {
    let Some(latest) = latest_of(update) else {
        return false;
    };
    let provider = update.source.provider.clone();
    let mut changed = if family.latest.get(&provider).map(String::as_str) == Some(latest) {
        false
    } else {
        family.latest.insert(provider.clone(), latest.to_string());
        true
    };
    match &update.latest_file_id {
        Some(file) => {
            if family.latest_file_ids.get(&provider) != Some(file) {
                family
                    .latest_file_ids
                    .insert(provider.clone(), file.clone());
                changed = true;
            }
        }
        None => changed |= family.latest_file_ids.remove(&provider).is_some(),
    }
    changed
}

/// Drops an ignore that no longer names the version the provider lists. A failed check leaves it alone, and so does
/// a check that brought no version.
fn drop_stale_ignore(family: &mut Family, update: &SourceUpdate) -> bool {
    let Some(latest) = latest_of(update) else {
        return false;
    };
    let provider = &update.source.provider;
    let Some(ignored) = family.ignored_versions.get(provider) else {
        return false;
    };
    if ignored.matches(Some(latest), update.latest_file_id.as_deref()) {
        return false;
    }
    family.ignored_versions.remove(provider);
    true
}

/// Replaces the requirements the provider listed for its newest file. The ones the user typed stay. A listed
/// requirement the user ignored stays ignored after the check.
fn apply_requirements(
    family: &mut Family,
    provider: &ProviderId,
    requirements: &[crate::domain::source::SourceRequirement],
    dlcs: &[Vec<String>],
    sync: Option<&crate::domain::source::SourceSync>,
) {
    let ignored: BTreeSet<(ProviderId, String)> = family
        .requirements
        .iter()
        .filter(|requirement| {
            requirement.source.provider == *provider
                && requirement.from_source
                && requirement.ignored
        })
        .map(|requirement| {
            (
                requirement.source.provider.clone(),
                requirement.source.mod_id.clone(),
            )
        })
        .collect();
    family.requirements.retain(|requirement| {
        !(requirement.source.provider == *provider && requirement.from_source)
    });
    let mut known: BTreeSet<SourceRef> = family
        .requirements
        .iter()
        .map(|requirement| requirement.source.clone())
        .collect();
    for requirement in requirements {
        if requirement.source.provider != *provider || !known.insert(requirement.source.clone()) {
            continue;
        }
        let mut requirement = requirement.clone();
        if ignored.contains(&(
            requirement.source.provider.clone(),
            requirement.source.mod_id.clone(),
        )) {
            requirement.ignored = true;
        }
        family.requirements.push(requirement);
    }
    family.source_dlcs.insert(provider.clone(), dlcs.to_vec());
    match sync {
        Some(sync) => {
            family.source_sync.insert(provider.clone(), sync.clone());
        }
        None => {
            family.source_sync.remove(provider);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::source::{ProviderId, SourceMeta, SourceRequirement, SourceSync};
    use crate::domain::version::{IgnoredVersion, UpdateState};
    use crate::library::Family;

    fn nexus() -> ProviderId {
        ProviderId::new("nexus")
    }

    fn requirement(mod_id: &str, from_source: bool, ignored: bool) -> SourceRequirement {
        SourceRequirement {
            source: crate::domain::source::SourceRef::new("nexus", mod_id),
            name: format!("Mod {mod_id}"),
            from_source,
            group: None,
            ignored,
        }
    }

    #[test]
    fn a_read_replaces_the_listed_requirements_and_keeps_the_typed_ones() {
        let mut family = Family {
            requirements: vec![
                requirement("10", true, false),
                requirement("20", false, false),
                requirement("30", true, true),
            ],
            ..Family::default()
        };
        let sync = SourceSync {
            file_id: "7".into(),
            version_id: "v".into(),
            checked_at: 1,
            mod_level: true,
            older_version: None,
        };
        apply_requirements(
            &mut family,
            &nexus(),
            &[
                requirement("20", true, false),
                requirement("30", true, false),
                requirement("40", true, false),
                requirement("40", true, false),
            ],
            &[vec!["DLC".into()]],
            Some(&sync),
        );
        assert_eq!(
            family
                .requirements
                .iter()
                .map(|requirement| (
                    requirement.source.mod_id.as_str(),
                    requirement.from_source,
                    requirement.ignored
                ))
                .collect::<Vec<_>>(),
            [
                ("20", false, false),
                ("30", true, true),
                ("40", true, false)
            ]
        );
        assert_eq!(family.source_dlcs[&nexus()], [vec!["DLC".to_string()]]);
        assert_eq!(family.source_sync[&nexus()], sync);

        // A file with no requirements clears the listed ones and leaves the typed ones alone.
        apply_requirements(&mut family, &nexus(), &[], &[], None);
        assert_eq!(family.requirements, [requirement("20", false, false)]);
        assert!(family.source_dlcs[&nexus()].is_empty());
        assert!(!family.source_sync.contains_key(&nexus()));
    }

    #[test]
    fn page_data_creates_the_source_entry() {
        let mut family = Family::default();
        let update = SourceUpdate {
            family: "nexus:1".into(),
            source: crate::domain::source::SourceRef::new("nexus", "1"),
            current: None,
            latest: None,
            latest_file_id: None,
            state: UpdateState::Unknown,
            error: None,
            meta: Some(SourceMeta {
                name: Some("Example".into()),
                ..SourceMeta::default()
            }),
            requirements: None,
            dlcs: Vec::new(),
            sync: None,
            matched: None,
            matched_part: None,
        };
        let read = update.meta.as_ref().expect("the update carries a page");
        assert!(apply_meta(&mut family, &update.source, read, 1_730_000_000));
        let meta = family
            .source("nexus")
            .and_then(|source| source.meta.as_ref())
            .expect("the page is saved");
        assert_eq!(meta.name.as_deref(), Some("Example"));
        assert_eq!(meta.checked_at, Some(1_730_000_000));
        assert!(!apply_meta(
            &mut family,
            &update.source,
            read,
            1_730_000_000
        ));
        // A later read is a later check, even when the page did not change.
        assert!(apply_meta(&mut family, &update.source, read, 1_731_000_000));
    }

    fn check(
        latest: Option<&str>,
        latest_file: Option<&str>,
        error: Option<AppError>,
    ) -> SourceUpdate {
        SourceUpdate {
            family: "nexus:1".into(),
            source: crate::domain::source::SourceRef::new("nexus", "1"),
            current: Some("1.1.3".into()),
            latest: latest.map(str::to_string),
            latest_file_id: latest_file.map(str::to_string),
            state: if latest.is_some() {
                UpdateState::Newer
            } else {
                UpdateState::Unknown
            },
            error,
            meta: None,
            requirements: None,
            dlcs: Vec::new(),
            sync: None,
            matched: None,
            matched_part: None,
        }
    }

    #[test]
    fn a_check_stores_the_newest_file_id_and_a_failed_check_keeps_it() {
        let provider = nexus();
        let mut family = Family::default();
        assert!(apply_latest(
            &mut family,
            &check(Some("1.1.3a"), Some("20"), None)
        ));
        assert_eq!(family.latest[&provider], "1.1.3a");
        assert_eq!(family.latest_file_ids[&provider], "20");
        // The same answer is not a change.
        assert!(!apply_latest(
            &mut family,
            &check(Some("1.1.3a"), Some("20"), None)
        ));
        // A failed check keeps the last good answer.
        assert!(!apply_latest(
            &mut family,
            &check(
                Some("2.0"),
                Some("30"),
                Some(AppError::new("nexus.bad_response"))
            )
        ));
        assert_eq!(family.latest[&provider], "1.1.3a");
        assert_eq!(family.latest_file_ids[&provider], "20");
        // A check that fell back to the page version drops the stale file id.
        assert!(apply_latest(
            &mut family,
            &check(Some("1.1.3a"), None, None)
        ));
        assert!(!family.latest_file_ids.contains_key(&provider));
    }

    #[test]
    fn an_ignore_survives_the_same_version_and_drops_when_another_one_appears() {
        let provider = nexus();
        let ignored = IgnoredVersion {
            version: "1.1.3a".into(),
            file_id: Some("20".into()),
        };
        let mut family = Family::default();
        family.ignored_versions.insert(provider.clone(), ignored);
        // The same version is still ignored.
        assert!(!drop_stale_ignore(
            &mut family,
            &check(Some("1.1.3a"), Some("20"), None)
        ));
        assert!(family.ignored_versions.contains_key(&provider));
        // A new version drops the old ignore.
        assert!(drop_stale_ignore(
            &mut family,
            &check(Some("1.1.3b"), Some("21"), None)
        ));
        assert!(!family.ignored_versions.contains_key(&provider));
    }
}
