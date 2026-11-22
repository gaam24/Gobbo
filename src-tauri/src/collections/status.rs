//! Where each member of a collection stands against the library and one profile. The library is indexed once; the
//! status is a pure read and never writes.

use std::collections::BTreeMap;

use crate::domain::collection::{
    CollectionCounts, CollectionItem, CollectionItemStatus, CollectionOrigin, CollectionPlan,
    CollectionSource, CollectionUpdatePolicy,
};
use crate::domain::library::view::FoundBy;
use crate::domain::library::{Library, LibraryView};
use crate::domain::profiles::Profile;

/// The statuses of every member and their counts.
pub struct StatusSet {
    pub statuses: BTreeMap<String, CollectionItemStatus>,
    pub counts: CollectionCounts,
}

impl StatusSet {
    /// The first member in plan order that still needs work. Unsupported and skipped members are not work.
    pub fn next<'a>(&self, plan: &'a CollectionPlan) -> Option<&'a CollectionItem> {
        plan.ordered().into_iter().find(|item| {
            !self
                .statuses
                .get(&item.key)
                .is_some_and(CollectionItemStatus::is_done)
                && !matches!(
                    self.statuses.get(&item.key),
                    Some(CollectionItemStatus::Unsupported)
                )
        })
    }
}

/// Computes the statuses of `plan` against `library`, `profile` and the collection's `origin`. A member is found in the
/// library by the shared rule (`LibraryView::find`) that packs and waiting entries use too.
pub fn statuses(
    plan: &CollectionPlan,
    library: &Library,
    profile: &Profile,
    origin: Option<&CollectionOrigin>,
) -> StatusSet {
    let view = LibraryView::new(library);
    statuses_with_view(plan, library, &view, profile, origin)
}

/// The same statuses with a view the caller already built. A collection operation checks the library for several
/// things (member statuses and exact files), so it can build the view once and share it instead of sorting the
/// library again for each question.
pub fn statuses_with_view(
    plan: &CollectionPlan,
    library: &Library,
    view: &LibraryView<'_>,
    profile: &Profile,
    origin: Option<&CollectionOrigin>,
) -> StatusSet {
    let mut statuses = BTreeMap::new();

    for item in &plan.items {
        statuses.insert(
            item.key.clone(),
            status(item, library, view, profile, origin),
        );
    }

    let counts = CollectionCounts::of(plan, &statuses);
    StatusSet { statuses, counts }
}

fn status(
    item: &CollectionItem,
    library: &Library,
    view: &LibraryView<'_>,
    profile: &Profile,
    origin: Option<&CollectionOrigin>,
) -> CollectionItemStatus {
    if origin.is_some_and(|origin| origin.excluded.iter().any(|key| key == &item.key)) {
        return CollectionItemStatus::Skipped;
    }

    if origin.is_some_and(|origin| origin.manual.iter().any(|key| key == &item.key)) {
        return CollectionItemStatus::Manual;
    }

    let provider = match &item.source {
        CollectionSource::Provider { .. } => true,
        CollectionSource::Bundle { .. } | CollectionSource::Direct { .. } => false,
        CollectionSource::Browse { .. } | CollectionSource::Manual => {
            return CollectionItemStatus::Missing;
        }
        CollectionSource::Unsupported { .. } => return CollectionItemStatus::Unsupported,
    };
    let wanted = item.wanted();

    if profile
        .waiting
        .iter()
        .any(|waiting| waiting.entry.is_same_mod(&wanted))
    {
        return CollectionItemStatus::Queued;
    }

    let Some(found) = view.find(&wanted, None) else {
        return CollectionItemStatus::Missing;
    };
    let listed = profile.mods.iter().any(|entry| entry.id == found.family);

    match found.by {
        // A provider member names its page: a local mod that only shares its name is not it.
        FoundBy::Name if provider => CollectionItemStatus::Missing,
        // A bundle or a direct file has no provider source, so its name is all that identifies it.
        FoundBy::Archive | FoundBy::File | FoundBy::Name if listed => {
            CollectionItemStatus::Installed
        }
        FoundBy::Archive | FoundBy::File | FoundBy::Name => CollectionItemStatus::InLibrary,
        FoundBy::Page => {
            let current = found
                .version
                .or_else(|| view.default_version(found.family))
                .map(|version| {
                    library
                        .version_string(version)
                        .map_or_else(|| version.folder.clone(), std::borrow::Cow::into_owned)
                })
                .unwrap_or_default();
            match (listed, item.update_policy) {
                (false, _) => CollectionItemStatus::InLibraryDifferentVersion { current },
                (true, CollectionUpdatePolicy::Exact) => {
                    CollectionItemStatus::DifferentVersion { current }
                }
                (true, CollectionUpdatePolicy::Latest | CollectionUpdatePolicy::Prefer) => {
                    CollectionItemStatus::Satisfied { current }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::collection::{
        CollectionInfo, CollectionItem, CollectionPlan, CollectionSource, CollectionUpdatePolicy,
        OPTIONAL_PHASE, PLAN_FORMAT,
    };
    use crate::domain::library::{Archive, Library, ModInfo, ModInfoEdit, ModVersion};
    use crate::domain::pack::PackMod;
    use crate::domain::profiles::{Entry, Profile};
    use crate::domain::source::ProviderId;
    use std::sync::Arc;

    fn item() -> CollectionItem {
        CollectionItem {
            key: "bundle:tag".into(),
            name: "Bundled Mod".into(),
            version: Some("1".into()),
            author: None,
            category: None,
            summary: String::new(),
            picture_url: None,
            optional: false,
            phase: 0,
            source: CollectionSource::Bundle {
                path: "Bundled Mod".into(),
                tag: Some("tag".into()),
            },
            file_name: None,
            size_kb: 0,
            md5: None,
            update_policy: CollectionUpdatePolicy::Exact,
            choices: Vec::new(),
            patches: BTreeMap::new(),
            instructions: String::new(),
            adult: false,
        }
    }

    fn plan() -> CollectionPlan {
        let item = item();
        CollectionPlan {
            format: PLAN_FORMAT,
            info: CollectionInfo {
                revision_label: String::new(),
                provider: "nexus".into(),
                game_key: "examplegame".into(),
                slug: "example".into(),
                revision: 1,
                latest_revision: 1,
                name: "Example".into(),
                author: None,
                summary: String::new(),
                description: String::new(),
                instructions: String::new(),
                picture_url: None,
                adult: false,
                total_size_kb: 0,
                mod_count: 1,
                game_versions: Vec::new(),
                changelog: None,
                url: String::new(),
            },
            items: vec![item.clone()],
            rules: Vec::new(),
            issues: Vec::new(),
            order: vec![item.key],
            has_bundles: true,
            has_patches: false,
            limited: false,
        }
    }

    fn library() -> Library {
        let mut library = Library::default();
        library.versions.insert(
            "Bundled Mod".into(),
            ModVersion {
                folder: "Bundled Mod".into(),
                family: "bundled".into(),
                slot: String::new(),
                files: Arc::from([]),
                size_bytes: 0,
                info: ModInfo {
                    name: "Bundled Mod".into(),
                    ..ModInfo::default()
                },
                edit: ModInfoEdit::default(),
                archive: None,
                parsed_sources: BTreeMap::new(),
                parsed_parts: BTreeMap::new(),
                installed_at: 0,
                fomod: Vec::new(),
                fomod_installer: false,
                integrity: None,
            },
        );
        library
    }

    #[test]
    fn a_bundled_member_without_a_provider_source_is_found_by_name() {
        let plan = plan();
        let library = library();
        let profile = Profile::new("p1".into(), "Default".into(), Vec::new());
        let first = statuses(&plan, &library, &profile, None);
        assert_eq!(
            first.statuses.get("bundle:tag"),
            Some(&CollectionItemStatus::InLibrary)
        );
        let mut installed = profile;
        installed.mods.push(Entry {
            id: "bundled".into(),
            folder: "Bundled Mod".into(),
            enabled: true,
            priority: None,
        });
        let second = statuses(&plan, &library, &installed, None);
        assert_eq!(
            second.statuses.get("bundle:tag"),
            Some(&CollectionItemStatus::Installed)
        );
    }

    /// Two files of one page are two members of a collection. An entry waiting for the second file must not make the
    /// first, already installed file look pending again. All ids are placeholders.
    #[test]
    fn a_waiting_sibling_file_does_not_hide_an_installed_member() {
        let provider = "test-provider";
        let mod_id = "test-mod";
        let first_file = "file-one";
        let second_file = "file-two";
        let family = format!("{provider}:{mod_id}");
        let first_key = family.clone();
        let second_key = format!("{family}#{second_file}");

        let addon = CollectionItem {
            key: first_key.clone(),
            name: "Placeholder addon".into(),
            version: Some("1.0".into()),
            author: None,
            category: None,
            summary: String::new(),
            picture_url: None,
            optional: true,
            phase: OPTIONAL_PHASE,
            source: CollectionSource::Provider {
                provider: provider.into(),
                mod_id: mod_id.into(),
                file_id: first_file.into(),
                part_id: None,
            },
            file_name: None,
            size_kb: 0,
            md5: None,
            update_policy: CollectionUpdatePolicy::Exact,
            choices: Vec::new(),
            patches: BTreeMap::new(),
            instructions: String::new(),
            adult: false,
        };
        let main = CollectionItem {
            key: second_key.clone(),
            name: "Placeholder main".into(),
            version: Some("2.0".into()),
            source: CollectionSource::Provider {
                provider: provider.into(),
                mod_id: mod_id.into(),
                file_id: second_file.into(),
                part_id: None,
            },
            ..addon.clone()
        };
        let mut plan = plan();
        plan.info.provider = provider.into();
        plan.items = vec![addon, main];
        plan.order = vec![first_key.clone(), second_key.clone()];

        let mut library = Library::default();
        let mut source_files = BTreeMap::new();
        source_files.insert(ProviderId::new(provider), first_file.to_string());
        library.archives.insert(
            "placeholder-digest".into(),
            Archive {
                original_name: "placeholder.zip".into(),
                file: "placeholder.zip".into(),
                named: None,
                source_files,
                source_parts: BTreeMap::new(),
                skipped: Vec::new(),
                skipped_total: 0,
            },
        );
        let mut parsed_sources = BTreeMap::new();
        parsed_sources.insert(ProviderId::new(provider), mod_id.to_string());
        library.versions.insert(
            "placeholder-folder".into(),
            ModVersion {
                folder: "placeholder-folder".into(),
                family: family.clone(),
                slot: String::new(),
                files: Arc::from([]),
                size_bytes: 0,
                info: ModInfo {
                    name: "Placeholder addon".into(),
                    version: Some("1.0".into()),
                    ..ModInfo::default()
                },
                edit: ModInfoEdit::default(),
                archive: Some("placeholder-digest".into()),
                parsed_sources,
                parsed_parts: BTreeMap::new(),
                installed_at: 0,
                fomod: Vec::new(),
                fomod_installer: false,
                integrity: None,
            },
        );

        let mut profile = Profile::new("p1".into(), "Default".into(), Vec::new());
        profile.mods.push(Entry {
            id: family.clone(),
            folder: "placeholder-folder".into(),
            enabled: true,
            priority: None,
        });
        let mut sources = BTreeMap::new();
        sources.insert(
            ProviderId::new(provider),
            crate::domain::source::SourceMod {
                mod_id: mod_id.into(),
                part_id: None,
                part_name: None,
                part_category: None,
                meta: Option::default(),
            },
        );
        let mut source_files = BTreeMap::new();
        source_files.insert(ProviderId::new(provider), second_file.to_string());
        profile.waiting.push(crate::domain::pack::WaitingMod {
            before: None,
            entry: PackMod {
                name: "Placeholder main".into(),
                version: Some("2.0".into()),
                family,
                enabled: true,
                priority: None,
                sources,
                source_files,
                source_parts: BTreeMap::new(),
                part_name: None,
                archive: None,
                fomod: Vec::new(),
                fomod_installer: false,
                tags: Vec::new(),
                instructions: String::new(),
                requires: Vec::new(),
                requirements: Vec::new(),
                source_dlcs: BTreeMap::new(),
                notes: String::new(),
                incompatible: Vec::new(),
                overrides: Vec::new(),
                steps: None,
            },
        });

        let result = statuses(&plan, &library, &profile, None);
        assert_eq!(
            result.statuses.get(&first_key),
            Some(&CollectionItemStatus::Installed)
        );
        assert_eq!(
            result.statuses.get(&second_key),
            Some(&CollectionItemStatus::Queued)
        );
    }
}
