//! The row and inspector details of one mod: its record, requirements, page and saved check.

use crate::context::size_kb;
use crate::domain::library::{Family, Library, LibraryView, ModInfo, ModVersion};
use crate::domain::mod_steps::ModSteps;
use crate::domain::profiles::{ProfilesView, entry::ProfileMod};
use crate::domain::source::{DownloadRoute, ProviderId};
use crate::domain::version::{update_state, with_ignored};
use crate::games::GameDef;
use crate::library::FamilyFields;
use crate::library::relations::{
    SourceLookup, UnmetRequirements, source_alternatives, with_source_name,
};
use crate::status::PlanOutcome;
use crate::status::views::{
    ModExtras, ModView, ProviderDlcs, SourceRequirementView, SourceSyncView, UpdateReport,
    description_preview,
};
use rustc_hash::FxHashMap;

use super::sources::{
    page_source, requirement_page_url, source_preview, source_summary, source_views,
};
use super::versions::archive_names;

/// The library and the indexes built from it, shared by every row of one status. Building the lookup from the same view
/// keeps the names, the sources and the enabled families consistent without passing each index to every row.
pub(in crate::status) struct LibraryContext<'a> {
    /// The parsed index.
    pub(in crate::status) library: &'a Library,
    /// The sorted versions and the lazily built source indexes.
    pub(in crate::status) view: &'a LibraryView<'a>,
    /// The enabled families and the names from the same view.
    pub(in crate::status) lookup: &'a SourceLookup<'a, 'a, 'a>,
}

/// One mod of the shown profile, with its record in the library, its source data and its conflicts.
pub(in crate::status) fn mod_view(
    index: usize,
    entry: &ProfileMod,
    game: &GameDef,
    context: &LibraryContext<'_>,
    profiles: &ProfilesView<'_>,
    plan: &PlanOutcome,
    requirements: &UnmetRequirements<'_>,
) -> ModView {
    let record = context.library.versions.get(&entry.folder);
    let family = context.library.families.get(&entry.id);
    let requires_names: Vec<String> = family.map_or_else(Vec::new, |family| {
        family
            .requires
            .iter()
            .map(|id| context.lookup.family_name(id).to_string())
            .collect()
    });
    let requirement_groups = family.map_or_else(Vec::new, requirement_groups);
    let (archive_name, parsed_archive) = record.map_or((None, None), |record| {
        archive_names(context.library, record)
    });
    // The row and its preview take their page data from the same source: the first with a page, or the first at all.
    let page = page_source(family);
    let page_id = page.map(|(provider, source)| format!("{provider}:{}", source.mod_id));
    let page_name = page
        .and_then(|(_, source)| source.meta.as_ref()?.name.clone())
        .or_else(|| family.map(|family| family.name.clone()));
    let part_id = page
        .and_then(|(_, source)| source.part_id.clone())
        .or_else(|| record.and_then(|record| record.parsed_parts.values().next().cloned()));
    let part_name = page
        .and_then(|(_, source)| source.part_name.clone())
        .or_else(|| record.map(|record| record.info.name.clone()));
    let part_category = page.and_then(|(_, source)| source.part_category.clone());
    let info = with_source_name(
        record.map_or_else(
            || ModInfo {
                name: entry.id.clone(),
                version: None,
                author: None,
                description: None,
            },
            |record| record.info.without_description(),
        ),
        &entry.folder,
        family.map(|family| family.name.as_str()),
        family.and_then(|family| {
            family
                .sources
                .values()
                .find_map(|source| source.meta.as_ref()?.name.as_deref())
        }),
        parsed_archive,
    );
    ModView {
        id: entry.id.clone(),
        folder: entry.folder.clone(),
        fomod_installer: record.is_some_and(|record| record.fomod_installer),
        version_count: u32::try_from(context.view.versions_of(&entry.id).len()).unwrap_or(u32::MAX),
        installed_at: record.map_or(0, |record| record.installed_at),
        slot: entry.slot.clone(),
        enabled: entry.enabled,
        priority: index as u32 + 1,
        load_priority: entry.priority,
        files: record.map_or(0, |record| record.files.len() as u32),
        size_kb: record.map_or(0, |record| size_kb(record.size_bytes)),
        wins: plan.wins.get(&entry.id).copied().unwrap_or(0),
        loses: plan.loses.get(&entry.id).copied().unwrap_or(0),
        archive: archive_name.map(str::to_string),
        archive_file: record
            .and_then(|record| record.archive.as_ref())
            .and_then(|sha256| context.library.archive_file_of(sha256)),
        skipped: record
            .and_then(|record| record.archive.as_ref())
            .and_then(|sha256| context.library.archives.get(sha256))
            .map_or(0, |archive| archive.skipped_total),
        info,
        source: source_preview(family),
        sources: source_views(family, game, context.library, record),
        page_id,
        page_name,
        part_id,
        part_name,
        part_category,
        update: update_report(family, record, context.view),
        tags: family
            .map(|family| family.tags.iter().cloned().collect())
            .unwrap_or_default(),
        instructions: family.map_or_else(String::new, |family| family.instructions.clone()),
        unmet_requirements: requirements.count(&entry.id),
        requires_names,
        requirement_groups,
        profiles: profiles
            .with_mod(&entry.id)
            .iter()
            .map(|profile| profile.name.clone())
            .collect(),
        collections: profiles
            .collections_of(&entry.id)
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
    }
}

/// The names of a family's provider requirements, grouped the way `sourceLists` groups them in the interface: each
/// list holds alternatives, and a requirement without a group is a list of its own. The order is kept.
fn requirement_groups(family: &Family) -> Vec<Vec<String>> {
    source_alternatives(&family.requirements)
        .into_iter()
        .map(|list| {
            list.into_iter()
                .map(|requirement| requirement.name.clone())
                .collect()
        })
        .collect()
}

/// The inputs shared by every row of one `mod_extras` batch: the library lookup, the saved mod steps and the routes of
/// the providers the batch names. The caller builds them once and passes them to each row.
pub(in crate::status) struct ExtrasContext<'a> {
    /// The library names and enabled families, read once for the whole batch.
    pub(in crate::status) lookup: &'a SourceLookup<'a, 'a, 'a>,
    /// The saved before/after steps of each mod.
    pub(in crate::status) mod_steps: &'a ModSteps,
    /// The routes of the providers named by the batch's requirements.
    pub(in crate::status) routes: &'a FxHashMap<ProviderId, DownloadRoute>,
}

/// Whether a saved account can fetch a provider's file directly, from the routes read once for the whole batch. A
/// provider missing from the map uses the owner's `Browser` fallback for an unknown provider.
fn direct_download(routes: &FxHashMap<ProviderId, DownloadRoute>, provider: &ProviderId) -> bool {
    crate::providers::route_or_browser(routes.get(provider).copied()) == DownloadRoute::Direct
}

/// The page text, notes and relation details that the list row leaves out. The table asks for the rows in its viewport,
/// and the inspector for the selected mod.
pub(in crate::status) fn mod_extras(
    folder: &str,
    family: Option<&crate::domain::library::Family>,
    record: Option<&ModVersion>,
    game: &GameDef,
    context: &ExtrasContext<'_>,
) -> ModExtras {
    let fields = FamilyFields::of(family);
    let incompatible_names = fields
        .incompatible
        .iter()
        .map(|id| context.lookup.family_name(id).to_string())
        .collect();
    let overrides_names = fields
        .overrides
        .iter()
        .map(|id| context.lookup.family_name(id).to_string())
        .collect();
    ModExtras {
        folder: folder.to_string(),
        info_description: record
            .and_then(|record| record.info.description.as_deref())
            .map(description_preview),
        source: source_summary(family),
        notes: fields.notes,
        requirements: family.map_or_else(Vec::new, |family| {
            family
                .requirements
                .iter()
                .map(|requirement| SourceRequirementView {
                    source: requirement.source.clone(),
                    name: requirement.name.clone(),
                    page_url: requirement_page_url(game, &requirement.source),
                    state: context.lookup.state_source(&requirement.source),
                    // The same route the planner and the download use: a saved account without direct downloads does
                    // not offer **Get**, even though the provider has the capability. The route was read once for the
                    // whole batch of rows, instead of once per requirement (a keyring access each time).
                    direct_download: direct_download(context.routes, &requirement.source.provider),
                    from_source: requirement.from_source,
                    group: requirement.group,
                    ignored: requirement.ignored,
                })
                .collect()
        }),
        source_dlcs: family.map_or_else(Vec::new, |family| {
            family
                .source_dlcs
                .iter()
                .map(|(provider, dlcs)| ProviderDlcs {
                    provider: provider.to_string(),
                    dlcs: dlcs.clone(),
                })
                .collect()
        }),
        source_sync: family.map_or_else(Vec::new, |family| {
            family
                .source_sync
                .iter()
                .map(|(provider, sync)| SourceSyncView {
                    provider: provider.to_string(),
                    checked_at: sync.checked_at,
                    older_version: sync.older_version.clone(),
                })
                .collect()
        }),
        fomod: record
            .map(|record| record.fomod.clone())
            .unwrap_or_default(),
        fomod_installer: record.is_some_and(|record| record.fomod_installer),
        requires: fields.requires,
        incompatible: fields.incompatible,
        incompatible_names,
        overrides: fields.overrides,
        overrides_names,
        steps_before: context
            .mod_steps
            .mods
            .get(folder)
            .map(|phases| phases.before_start.clone())
            .unwrap_or_default(),
        steps_after: context
            .mod_steps
            .mods
            .get(folder)
            .map(|phases| phases.after_end.clone())
            .unwrap_or_default(),
    }
}

/// The last saved check of a mod, with the version the profile selects now, so the result follows a version change. The
/// first source whose page a check read answers for the mod, in provider order. A page with no check time (an import
/// that did not read it, a pack that dropped the time) is not a check.
pub(in crate::status) fn update_report(
    family: Option<&crate::domain::library::Family>,
    version: Option<&ModVersion>,
    view: &LibraryView<'_>,
) -> Option<UpdateReport> {
    let family = family?;
    let (provider, source, meta) = family.sources.iter().find_map(|(provider, source)| {
        let meta = source
            .meta
            .as_ref()
            .filter(|meta| meta.checked_at.is_some())?;
        Some((provider, source, meta))
    })?;
    let has_part = family
        .sources
        .get(provider)
        .and_then(|source| source.part_id.as_deref())
        .is_some();
    let latest = family
        .latest
        .get(provider)
        .cloned()
        .or_else(|| if has_part { None } else { meta.version.clone() });
    let latest_file = family.latest_file_ids.get(provider).map(String::as_str);
    let current = version.and_then(|version| version.info.version.clone());
    let current_file = version.and_then(|version| view.file_id_of(version, provider.as_str()));
    let state = update_state(
        latest.as_deref(),
        latest_file,
        current.as_deref(),
        current_file,
    );
    let state = with_ignored(
        state,
        latest.as_deref(),
        latest_file,
        family.ignored_versions.get(provider),
    );
    Some(
        UpdateReport {
            id: family.id.clone(),
            provider: provider.to_string(),
            mod_id: source.mod_id.clone(),
            current,
            latest,
            latest_file_id: latest_file.map(str::to_string),
            state,
            latest_installed: false,
            installed_folder: None,
            has_update: false,
            error: None,
        }
        .with_library(view, latest_file),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::source::SourceRef;

    /// The grouping rule as it was written before the shared owner existed, kept only as a reference for the
    /// equivalence test below. A requirement without a group is its own list; a provider and group pair names one
    /// list of alternatives, in first-seen order.
    fn old_requirement_groups(family: &Family) -> Vec<Vec<String>> {
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut keys: Vec<Option<(String, u32)>> = Vec::new();

        for requirement in &family.requirements {
            let key = requirement
                .group
                .map(|group| (requirement.source.provider.to_string(), group));
            let known = key
                .as_ref()
                .and_then(|key| keys.iter().position(|known| known.as_ref() == Some(key)));
            if let Some(index) = known {
                groups[index].push(requirement.name.clone());
                continue;
            }

            keys.push(key);
            groups.push(vec![requirement.name.clone()]);
        }

        groups
    }

    fn requirement(
        name: &str,
        provider: &str,
        id: &str,
        group: Option<u32>,
    ) -> crate::domain::source::SourceRequirement {
        crate::domain::source::SourceRequirement {
            source: SourceRef::new(provider, id),
            name: name.into(),
            from_source: false,
            group,
            ignored: false,
        }
    }

    #[test]
    fn requirement_groups_match_the_shared_grouping_rule() {
        let family = Family {
            requirements: vec![
                requirement("ungrouped-a", "provider-a", "1", None),
                requirement("group-a-1", "provider-a", "2", Some(1)),
                requirement("group-b-1", "provider-b", "3", Some(1)),
                requirement("ungrouped-b", "provider-b", "4", None),
                requirement("group-a-2", "provider-a", "5", Some(1)),
                requirement("group-b-2", "provider-b", "6", Some(2)),
                requirement("group-c-1", "provider-a", "7", Some(2)),
            ],
            ..Family::default()
        };
        let expected: Vec<Vec<String>> = vec![
            vec!["ungrouped-a".into()],
            vec!["group-a-1".into(), "group-a-2".into()],
            vec!["group-b-1".into()],
            vec!["ungrouped-b".into()],
            vec!["group-b-2".into()],
            vec!["group-c-1".into()],
        ];

        assert_eq!(old_requirement_groups(&family), expected);
        assert_eq!(requirement_groups(&family), expected);
    }

    #[test]
    fn a_direct_route_is_used_only_for_a_provider_in_the_map() {
        let routes = FxHashMap::from_iter([(ProviderId::new("provider-a"), DownloadRoute::Direct)]);
        assert!(direct_download(&routes, &ProviderId::new("provider-a")));
        assert!(!direct_download(&routes, &ProviderId::new("provider-b")));

        let browser =
            FxHashMap::from_iter([(ProviderId::new("provider-a"), DownloadRoute::Browser)]);
        assert!(!direct_download(&browser, &ProviderId::new("provider-a")));
    }
}
