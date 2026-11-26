use std::sync::OnceLock;

use rustc_hash::{FxBuildHasher, FxHashMap};

use crate::domain::library::Library;
use crate::domain::library::ModVersion;
use crate::domain::naming::match_key;
use crate::domain::pathkey::fold_case;
use crate::domain::source::ProviderId;
use crate::domain::version;

use super::PageFamiliesIter;

/// The versions of each family with the key they are sorted by.
type VersionsByFamily<'a> =
    FxHashMap<&'a str, Vec<(&'a ModVersion, Option<std::borrow::Cow<'a, str>>)>>;

/// The families of one source page. Most pages have one family, so the first stays inline and only a real group pays
/// for a `Vec`; a page entry per family was one allocation per mod.
enum PageFamilies<'a> {
    One(&'a str),
    Many(Vec<&'a str>),
}

impl<'a> PageFamilies<'a> {
    fn push(&mut self, family: &'a str) {
        match self {
            Self::One(first) if *first == family => {}
            Self::One(first) => *self = Self::Many(vec![*first, family]),
            Self::Many(families) if !families.contains(&family) => families.push(family),
            Self::Many(_) => {}
        }
    }

    fn finish(self) -> Self {
        match self {
            Self::One(family) => Self::One(family),
            Self::Many(mut families) => {
                families.sort_unstable();
                families.dedup();
                Self::Many(families)
            }
        }
    }
}

/// Indexes over one `Library`. The view borrows the library; it is cheap to build relative to one status or update
/// check, and it must not outlive the library it was made from.
pub struct LibraryView<'a> {
    // The sibling `find` module reads the library directly when it resolves a wanted mod.
    pub(super) library: &'a Library,
    by_family: FxHashMap<&'a str, Vec<&'a ModVersion>>,
    default_version: FxHashMap<&'a str, &'a ModVersion>,
    // The indexes below are built on their first lookup: the status asks only for the versions of each family, and a
    // map it never reads was about a tenth of every status.
    by_source: OnceLock<FxHashMap<(&'a str, &'a str), &'a str>>,
    by_part: OnceLock<FxHashMap<(&'a str, &'a str, &'a str), &'a str>>,
    /// Provider first, then the page id: nested maps let a lookup borrow the key for the call only, so the returned
    /// iterator depends on the view alone.
    by_page: OnceLock<FxHashMap<&'a str, FxHashMap<&'a str, PageFamilies<'a>>>>,
    by_name: OnceLock<FxHashMap<&'a str, &'a str>>,
    /// Provider ids folded with the app's case rule, for community profile slugs.
    by_source_folded: OnceLock<FxHashMap<(&'a str, String), &'a str>>,
    /// Letter-and-digit folder keys, for Mod Organizer 2 lists.
    by_key: OnceLock<FxHashMap<String, &'a ModVersion>>,
    /// Built on the first archive lookup. The status never asks for it, so it must not cost one `Vec` per archive.
    by_archive: OnceLock<FxHashMap<&'a str, Vec<&'a ModVersion>>>,
}

impl<'a> LibraryView<'a> {
    /// Builds the versions of each family, in the order the pickers show them. The source, part, page and name indexes
    /// wait for their first lookup.
    pub fn new(library: &'a Library) -> Self {
        let mut grouped: VersionsByFamily<'a> =
            FxHashMap::with_capacity_and_hasher(library.families.len(), FxBuildHasher);
        for version in library.versions.values() {
            grouped
                .entry(version.family.as_str())
                .or_default()
                .push((version, library.version_string(version)));
        }

        let mut by_family = FxHashMap::with_capacity_and_hasher(grouped.len(), FxBuildHasher);
        let mut default_version = FxHashMap::with_capacity_and_hasher(grouped.len(), FxBuildHasher);

        for (family, mut versions) in grouped {
            versions.sort_unstable_by(|(left, left_key), (right, right_key)| {
                version::compare_display(
                    left_key.as_deref(),
                    left.installed_at,
                    &left.folder,
                    right_key.as_deref(),
                    right.installed_at,
                    &right.folder,
                )
            });

            if let Some((first, _)) = versions.first() {
                default_version.insert(family, *first);
            }

            by_family.insert(
                family,
                versions.into_iter().map(|(version, _)| version).collect(),
            );
        }

        Self {
            library,
            by_family,
            default_version,
            by_source: OnceLock::new(),
            by_part: OnceLock::new(),
            by_page: OnceLock::new(),
            by_name: OnceLock::new(),
            by_source_folded: OnceLock::new(),
            by_key: OnceLock::new(),
            by_archive: OnceLock::new(),
        }
    }

    /// Calls `visit` with every source the library names, as (provider, mod id, part, family): first the sources of the
    /// families, in family-id order, then the ids a version knows before its family does. An index that keeps the first
    /// entry for a key therefore lets the family win over a version, and the first family id win over the others.
    fn for_each_source(&self, mut visit: impl FnMut(&'a str, &'a str, Option<&'a str>, &'a str)) {
        for (id, known) in &self.library.families {
            for (provider, source) in &known.sources {
                visit(
                    provider.as_str(),
                    source.mod_id.as_str(),
                    source.part_id.as_deref(),
                    id.as_str(),
                );
            }
        }

        for version in self.library.versions.values() {
            for (provider, mod_id) in &version.parsed_sources {
                let part = version.parsed_parts.get(provider).map(String::as_str);
                visit(
                    provider.as_str(),
                    mod_id.as_str(),
                    part,
                    version.family.as_str(),
                );
            }
        }
    }

    fn by_source(&self) -> &FxHashMap<(&'a str, &'a str), &'a str> {
        self.by_source.get_or_init(|| {
            let mut map = FxHashMap::default();
            self.for_each_source(|provider, mod_id, _, family| {
                map.entry((provider, mod_id)).or_insert(family);
            });
            map
        })
    }

    fn by_part(&self) -> &FxHashMap<(&'a str, &'a str, &'a str), &'a str> {
        self.by_part.get_or_init(|| {
            let mut map = FxHashMap::default();
            self.for_each_source(|provider, mod_id, part, family| {
                if let Some(part) = part {
                    map.entry((provider, mod_id, part)).or_insert(family);
                }
            });
            map
        })
    }

    fn by_page(&self) -> &FxHashMap<&'a str, FxHashMap<&'a str, PageFamilies<'a>>> {
        self.by_page.get_or_init(|| {
            let mut map: FxHashMap<&'a str, FxHashMap<&'a str, PageFamilies<'a>>> =
                FxHashMap::default();
            self.for_each_source(|provider, mod_id, _, family| {
                map.entry(provider)
                    .or_default()
                    .entry(mod_id)
                    .or_insert(PageFamilies::One(family))
                    .push(family);
            });
            // A page may have gained the same family from both the family and a version; keep it once.
            for pages in map.values_mut() {
                for families in pages.values_mut() {
                    let built = std::mem::replace(families, PageFamilies::Many(Vec::new()));
                    *families = built.finish();
                }
            }

            map
        })
    }

    /// The version in one library folder.
    pub fn version(&self, folder: &str) -> Option<&'a ModVersion> {
        self.library.versions.get(folder)
    }

    /// Versions of one family, newest first.
    pub fn versions_of(&self, family: &str) -> &[&'a ModVersion] {
        self.by_family.get(family).map_or(&[], Vec::as_slice)
    }

    /// The version a profile uses when it has no choice yet: the newest one.
    pub fn default_version(&self, family: &str) -> Option<&'a ModVersion> {
        self.default_version.get(family).copied()
    }

    /// Whether an installed version is `wanted`, by its `info.json` version or by the version in the name of the
    /// archive it came from.
    pub fn version_matches(&self, version: &ModVersion, wanted: &str) -> bool {
        version.info.version.as_deref() == Some(wanted)
            || self
                .library
                .archive_label_of(version)
                .is_some_and(|label| label.version == wanted)
    }

    /// The provider's file id of an installed version, when the archive it came from kept one.
    pub fn file_id_of(&self, version: &ModVersion, provider: &str) -> Option<&str> {
        version
            .archive
            .as_ref()
            .and_then(|sha256| self.library.archives.get(sha256))
            .and_then(|archive| archive.source_files.get(provider))
            .map(String::as_str)
    }

    /// The installed folder of the family's latest version: the one whose archive has the provider's `latest_file`
    /// when the check named a file, otherwise the one whose version is `latest`. A file id is exact, so it never falls
    /// back to the version string on a page with several parts.
    pub fn installed_latest_folder(
        &self,
        family: &str,
        provider: &str,
        latest_file: Option<&str>,
        latest: Option<&str>,
    ) -> Option<&str> {
        self.versions_of(family)
            .iter()
            .find(|version| {
                latest_file.map_or_else(
                    || latest.is_some_and(|wanted| self.version_matches(version, wanted)),
                    |file| self.file_id_of(version, provider) == Some(file),
                )
            })
            .map(|version| version.folder.as_str())
    }

    /// The installed version of one exact provider file, when the library has it. A page can carry several parts, so
    /// the file id, not the version label or the family, decides which version this is.
    pub fn version_of_file(
        &self,
        provider: &ProviderId,
        mod_id: &str,
        file_id: &str,
    ) -> Option<&'a ModVersion> {
        self.families_of_page(provider, mod_id)
            .flat_map(|family| self.versions_of(family))
            .find(|version| self.file_id_of(version, provider.as_str()) == Some(file_id))
            .copied()
    }

    /// The version the user ignored for this provider, when the family has one.
    pub fn ignored_version(
        &self,
        family: &str,
        provider: &ProviderId,
    ) -> Option<&crate::domain::version::IgnoredVersion> {
        self.library
            .families
            .get(family)?
            .ignored_versions
            .get(provider)
    }

    /// The family that names this mod on the provider. When two families share a source, the first family id wins.
    pub fn family_of_source(&self, provider: &ProviderId, mod_id: &str) -> Option<&'a str> {
        self.by_source().get(&(provider.as_str(), mod_id)).copied()
    }

    /// The family whose provider id matches after the app's case fold. Only the families' own sources count: a
    /// community profile names a package by its id, never by a display name or a version's parsed source.
    pub(crate) fn family_of_source_folded(
        &self,
        provider: &ProviderId,
        mod_id: &str,
    ) -> Option<&'a str> {
        self.by_source_folded
            .get_or_init(|| {
                let mut map = FxHashMap::default();
                for (id, known) in &self.library.families {
                    for (provider, source) in &known.sources {
                        map.entry((provider.as_str(), fold_case(&source.mod_id)))
                            .or_insert(id.as_str());
                    }
                }
                map
            })
            .get(&(provider.as_str(), fold_case(mod_id)))
            .copied()
    }

    /// The first version whose folder has this letter-and-digit key, in folder order: the rule a Mod Organizer 2 list
    /// uses, where the first folder of a key wins.
    pub(crate) fn version_by_key(&self, key: &str) -> Option<&'a ModVersion> {
        self.by_key
            .get_or_init(|| {
                let mut map = FxHashMap::default();
                for version in self.library.versions.values() {
                    map.entry(match_key(&version.folder)).or_insert(version);
                }
                map
            })
            .get(key)
            .copied()
    }

    /// The family of one part of a page, when the library has it.
    pub fn family_of_part(
        &self,
        provider: &ProviderId,
        mod_id: &str,
        part_id: &str,
    ) -> Option<&'a str> {
        self.by_part()
            .get(&(provider.as_str(), mod_id, part_id))
            .copied()
    }

    /// Every family of one page, in family-id order. The iterator borrows the view and yields the families themselves;
    /// a requirement check does not need a `Vec`.
    pub fn families_of_page<'b>(
        &'b self,
        provider: &ProviderId,
        mod_id: &str,
    ) -> PageFamiliesIter<'a, 'b> {
        match self
            .by_page()
            .get(provider.as_str())
            .and_then(|pages| pages.get(mod_id))
        {
            Some(PageFamilies::One(family)) => PageFamiliesIter {
                one: Some(family),
                many: [].iter(),
            },
            Some(PageFamilies::Many(families)) => PageFamiliesIter {
                one: None,
                many: families.iter(),
            },
            None => PageFamiliesIter {
                one: None,
                many: [].iter(),
            },
        }
    }

    /// The family with this name. When two families share a name, the first id (by family id) wins, as a scan of the
    /// family map in order did.
    pub fn family_named(&self, name: &str) -> Option<&'a str> {
        self.by_name
            .get_or_init(|| {
                let mut map = FxHashMap::default();
                // The families map is sorted by id, so the first family with a name wins.
                for (id, known) in &self.library.families {
                    map.entry(known.name.as_str()).or_insert(id.as_str());
                }

                map
            })
            .get(name)
            .copied()
    }

    /// The name of a family, or `None` when the library has no such family.
    pub fn family_name(&self, family: &str) -> Option<&'a str> {
        self.library
            .families
            .get(family)
            .map(|family| family.name.as_str())
    }

    /// The versions whose archive had this SHA-256 sum, in folder order.
    pub fn versions_from_archive(&self, sha256: &str) -> &[&'a ModVersion] {
        let by_archive = self.by_archive.get_or_init(|| {
            let mut map: FxHashMap<&'a str, Vec<&'a ModVersion>> = FxHashMap::default();

            for version in self.library.versions.values() {
                if let Some(sha256) = version.archive.as_deref() {
                    map.entry(sha256).or_default().push(version);
                }
            }

            map
        });
        by_archive.get(sha256).map_or(&[], Vec::as_slice)
    }
}
