//! The one rule that answers "which mod of the library is this?". A pack mod, a waiting entry and a collection member
//! all describe the mod they want as a `PackMod`; every caller asks this rule instead of keeping its own index, so a
//! fix to the matching reaches packs, collections and the waiting list at once. A profile importer names the same
//! description with a different comparison policy, which this rule owns too.

use std::collections::BTreeMap;

use super::LibraryView;
use crate::domain::library::ModVersion;
use crate::domain::naming::match_key;
use crate::domain::pack::{PackArchive, PackMod};
use crate::domain::pathkey::fold_case;
use crate::domain::source::{ProviderId, SourceMod};

/// How the library mod was recognised, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundBy {
    /// The version came from the archive the mod names, by its SHA-256 sum.
    Archive,
    /// The version came from the exact provider file the mod names.
    File,
    /// The family is the provider page (and part) the mod names; the file may differ.
    Page,
    /// The family or folder has the mod's name; nothing else identifies it.
    Name,
}

/// How a caller's identifiers are compared with the library's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MatchPolicy {
    /// Exact ids and ASCII-case-insensitive names: the rule of packs, collections and waiting entries.
    #[default]
    Strict,
    /// Provider page ids and version labels compared with the app's case fold, with no fallback to a name: the rule of
    /// a community profile's package slug.
    FoldCase,
    /// Names compared by their letter-and-digit key against version folders: the rule of a Mod Organizer 2 list.
    MatchKey,
}

static NO_SOURCES: BTreeMap<ProviderId, SourceMod> = BTreeMap::new();
static NO_FILES: BTreeMap<ProviderId, String> = BTreeMap::new();
static NO_PARTS: BTreeMap<ProviderId, String> = BTreeMap::new();

/// The description the matcher resolves. `find` builds this from a pack mod; an importer that compares identifiers
/// differently builds the same query with its own policy.
pub struct FindQuery<'a> {
    /// The name the caller knows the mod by.
    pub name: &'a str,
    /// The version label the caller names, when it names one.
    pub version: Option<&'a str>,
    /// The archive the mod came from, when the caller names one.
    pub archive: Option<&'a PackArchive>,
    /// The provider pages the caller names.
    pub sources: &'a BTreeMap<ProviderId, SourceMod>,
    /// The exact provider files the caller names.
    pub source_files: &'a BTreeMap<ProviderId, String>,
    /// The parts the caller names, per provider.
    pub source_parts: &'a BTreeMap<ProviderId, String>,
    /// The part's display name, when the caller names it.
    pub part_name: Option<&'a str>,
    /// The page a community or other source-based importer names, when it names one.
    pub provider: Option<&'a ProviderId>,
    /// The mod id of that page.
    pub mod_id: Option<&'a str>,
    /// How the identifiers are compared.
    pub policy: super::MatchPolicy,
}

impl<'a> FindQuery<'a> {
    /// The strict query of a pack mod, a collection member or a waiting entry.
    pub(crate) fn of(wanted: &'a PackMod) -> Self {
        Self {
            name: &wanted.name,
            version: wanted.version.as_deref(),
            archive: wanted.archive.as_ref(),
            sources: &wanted.sources,
            source_files: &wanted.source_files,
            source_parts: &wanted.source_parts,
            part_name: wanted.part_name.as_deref(),
            provider: None,
            mod_id: None,
            policy: MatchPolicy::Strict,
        }
    }

    /// A community profile's package slug and version, compared with the app's case fold.
    pub fn folded(provider: &'a ProviderId, mod_id: &'a str, version: &'a str) -> Self {
        Self {
            name: mod_id,
            version: Some(version),
            archive: None,
            sources: &NO_SOURCES,
            source_files: &NO_FILES,
            source_parts: &NO_PARTS,
            part_name: None,
            provider: Some(provider),
            mod_id: Some(mod_id),
            policy: MatchPolicy::FoldCase,
        }
    }

    /// A Mod Organizer 2 folder name, compared with the letter-and-digit key of a library folder.
    pub fn by_key(name: &'a str) -> Self {
        Self {
            name,
            version: None,
            archive: None,
            sources: &NO_SOURCES,
            source_files: &NO_FILES,
            source_parts: &NO_PARTS,
            part_name: None,
            provider: None,
            mod_id: None,
            policy: MatchPolicy::MatchKey,
        }
    }
}

/// The library family a wanted mod stands for.
#[derive(Debug, Clone, Copy)]
pub struct Found<'a> {
    pub family: &'a str,
    /// The version the wanted mod names: the same archive, the same provider file, or the same version label. With no
    /// version wanted, the family's newest one. `None` when the family has the mod in another version only.
    pub version: Option<&'a ModVersion>,
    pub by: FoundBy,
}

impl Found<'_> {
    /// The family and folder as owned strings, for a profile entry. `None` when the wanted version is missing.
    pub fn entry(&self) -> Option<(String, String)> {
        self.version
            .map(|version| (self.family.to_string(), version.folder.clone()))
    }
}

impl<'a> LibraryView<'a> {
    /// Finds the library mod `wanted` describes. This answers "which library family does this wanted mod match?": the
    /// archive digest wins, then the exact provider file, then the page and its part, then a trimmed, case-insensitive
    /// name. `imported` names the folders a batch has just unpacked, by archive digest: a pack that brought its own
    /// archives resolves to those folders instead of an older copy of the same archive.
    ///
    /// The other two questions stay with their owners: "the same provider file?" is
    /// `crate::domain::source::SourceRef::same_file` and "the same mod?" is `crate::domain::pack::PackMod::is_same_mod`.
    /// Those compare two descriptions; this one resolves a description against the library's families.
    pub fn find(
        &self,
        wanted: &PackMod,
        imported: Option<&BTreeMap<String, Vec<String>>>,
    ) -> Option<Found<'a>> {
        self.find_with(FindQuery::of(wanted), imported)
    }

    /// Finds the library mod `query` describes under its comparison policy. The strict policy is the one `find` uses;
    /// the other two let a profile importer keep its own matching rule without scanning the library itself.
    pub fn find_with(
        &self,
        query: FindQuery<'_>,
        imported: Option<&BTreeMap<String, Vec<String>>>,
    ) -> Option<Found<'a>> {
        match query.policy {
            MatchPolicy::Strict => self.find_strict(&query, imported),
            MatchPolicy::FoldCase => self.find_folded(&query),
            MatchPolicy::MatchKey => self.find_by_key(&query),
        }
    }

    fn find_strict(
        &self,
        query: &FindQuery<'_>,
        imported: Option<&BTreeMap<String, Vec<String>>>,
    ) -> Option<Found<'a>> {
        if let Some(found) = self.by_archive(query, imported) {
            return Some(found);
        }
        // The provider's exact file id is stronger than the version label: a recipe can name a version differently from
        // the archive it points at.
        for (provider, source) in query.sources {
            let Some(file_id) = query.source_files.get(provider) else {
                continue;
            };

            if let Some(version) = self.version_of_file(provider, &source.mod_id, file_id) {
                return Some(Found {
                    family: version.family.as_str(),
                    version: Some(version),
                    by: FoundBy::File,
                });
            }
        }

        let (family, by) = self
            .family_of_wanted_page(query)
            .map(|family| (family, FoundBy::Page))
            .or_else(|| {
                self.family_named_loosely(query.name)
                    .map(|family| (family, FoundBy::Name))
            })?;
        let version = query.version.map_or_else(
            || self.default_version(family),
            |label| {
                self.versions_of(family)
                    .iter()
                    .find(|version| self.version_matches(version, label))
                    .copied()
            },
        );
        Some(Found {
            family,
            version,
            by,
        })
    }

    /// The community profile's rule: a package slug is a provider id, never a display name, and both the id and the
    /// version are compared with the app's case fold. No exact version leaves the choice to the caller, which selects
    /// the family's default and reports the other version.
    fn find_folded(&self, query: &FindQuery<'_>) -> Option<Found<'a>> {
        let family = self.family_of_source_folded(query.provider?, query.mod_id?)?;
        let version = query.version.and_then(|label| {
            let wanted = fold_case(label);
            self.library.versions.values().find(|version| {
                version.family == family
                    && version
                        .info
                        .version
                        .as_deref()
                        .is_some_and(|known| fold_case(known) == wanted)
            })
        });
        Some(Found {
            family,
            version,
            by: FoundBy::Page,
        })
    }

    /// The Mod Organizer 2 rule: a list line names the folder, so the matching version is the one whose folder key is
    /// the line's key. The first folder of a key wins, and the caller keeps its own filter of one version per family.
    fn find_by_key(&self, query: &FindQuery<'_>) -> Option<Found<'a>> {
        let version = self.version_by_key(&match_key(query.name))?;
        Some(Found {
            family: version.family.as_str(),
            version: Some(version),
            by: FoundBy::Name,
        })
    }

    fn by_archive(
        &self,
        query: &FindQuery<'_>,
        imported: Option<&BTreeMap<String, Vec<String>>>,
    ) -> Option<Found<'a>> {
        let sha256 = &query.archive?.sha256;
        let from_archive = self.versions_from_archive(sha256);
        let fresh: Vec<&'a ModVersion> = imported
            .and_then(|map| map.get(sha256.as_str()))
            .map(|folders| {
                from_archive
                    .iter()
                    .filter(|version| folders.contains(&version.folder))
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        let candidates = if fresh.is_empty() {
            from_archive
        } else {
            &fresh
        };
        let version = candidates
            .iter()
            .find(|version| version.info.name == query.name)
            .or_else(|| candidates.first())
            .copied()?;
        Some(Found {
            family: version.family.as_str(),
            version: Some(version),
            by: FoundBy::Archive,
        })
    }

    /// The family of the page and part `query` names. Without a part id, the part whose name the mod carries wins,
    /// then the page's first family, so two addons of one page do not resolve to the same family by accident.
    fn family_of_wanted_page(&self, query: &FindQuery<'_>) -> Option<&'a str> {
        query
            .sources
            .iter()
            .find_map(|(provider, source)| {
                let part = source
                    .part_id
                    .as_deref()
                    .or_else(|| query.source_parts.get(provider).map(String::as_str))?;
                self.family_of_part(provider, &source.mod_id, part)
            })
            .or_else(|| {
                let name = query.part_name.unwrap_or(query.name).trim();
                query.sources.iter().find_map(|(provider, source)| {
                    let mut families = self.families_of_page(provider, &source.mod_id);
                    let first = families.next();
                    families
                        .find(|family| {
                            self.family_name(family)
                                .is_some_and(|known| known.trim().eq_ignore_ascii_case(name))
                        })
                        .or(first)
                })
            })
    }

    /// The family with this name: the exact family name first, then a family, a version or a folder with that name,
    /// with case and outer spaces ignored.
    fn family_named_loosely(&self, name: &str) -> Option<&'a str> {
        self.family_named(name).or_else(|| {
            let name = name.trim();
            let same = |text: &str| text.trim().eq_ignore_ascii_case(name);
            self.library
                .families
                .iter()
                .find(|(_, family)| same(&family.name))
                .map(|(id, _)| id.as_str())
                .or_else(|| {
                    self.library
                        .versions
                        .values()
                        .find(|version| same(&version.info.name) || same(&version.folder))
                        .map(|version| version.family.as_str())
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::library::{Archive, Family, Library};
    use crate::domain::source::{ProviderId, SourceMod};
    use crate::test_support::mod_version;

    const PROVIDER: &str = "example-provider";

    fn source(mod_id: &str, part: Option<&str>) -> SourceMod {
        SourceMod {
            mod_id: mod_id.into(),
            part_id: part.map(str::to_string),
            part_name: None,
            part_category: None,
            meta: None,
        }
    }

    fn family(id: &str, name: &str, part: Option<&str>) -> Family {
        let mut family = Family::of(&mod_version(id, id, name, None));
        family.sources = BTreeMap::from([(ProviderId::new(PROVIDER), source("page", part))]);
        family
    }

    /// One page with two parts, each its own family; the second part's version came from file `file-b`.
    fn library() -> Library {
        let mut library = Library::default();

        for (id, name, part) in [
            ("fam-a", "Main File", "part-a"),
            ("fam-b", "Addon File", "part-b"),
        ] {
            library
                .families
                .insert(id.into(), family(id, name, Some(part)));
            let mut version = mod_version(id, id, name, Some("1.0"));
            version.archive = Some(format!("sha-{id}"));
            library.versions.insert(id.into(), version);
            library.archives.insert(
                format!("sha-{id}"),
                Archive {
                    original_name: format!("{id}.zip"),
                    file: format!("{id}.zip"),
                    named: None,
                    source_files: BTreeMap::from([(
                        ProviderId::new(PROVIDER),
                        format!("file-{id}"),
                    )]),
                    source_parts: BTreeMap::new(),
                    skipped: Vec::new(),
                    skipped_total: 0,
                },
            );
        }

        library
    }

    fn wanted(name: &str, part: Option<&str>, file: Option<&str>) -> PackMod {
        let mut entry: PackMod = serde_json::from_str(&format!(
            r#"{{"name":"{name}","version":null,"family":"wanted","enabled":true}}"#
        ))
        .unwrap();
        entry.sources = BTreeMap::from([(ProviderId::new(PROVIDER), source("page", part))]);

        if let Some(file) = file {
            entry.source_files = BTreeMap::from([(ProviderId::new(PROVIDER), file.to_string())]);
        }

        entry
    }

    #[test]
    fn an_exact_file_wins_over_the_page() {
        let library = library();
        let view = LibraryView::new(&library);
        let found = view
            .find(&wanted("Main File", None, Some("file-fam-b")), None)
            .unwrap();
        assert_eq!((found.family, found.by), ("fam-b", FoundBy::File));
    }

    #[test]
    fn a_part_id_picks_its_own_family_of_a_shared_page() {
        let library = library();
        let view = LibraryView::new(&library);
        let found = view
            .find(&wanted("Unrelated", Some("part-b"), Some("file-new")), None)
            .unwrap();
        assert_eq!((found.family, found.by), ("fam-b", FoundBy::Page));
    }

    #[test]
    fn without_a_part_the_family_with_the_wanted_name_wins() {
        let library = library();
        let view = LibraryView::new(&library);
        let found = view.find(&wanted("addon file", None, None), None).unwrap();
        assert_eq!(found.family, "fam-b");
    }

    #[test]
    fn a_mod_without_a_source_is_found_by_its_name_ignoring_case() {
        let library = library();
        let view = LibraryView::new(&library);
        let mut entry = wanted(" MAIN file ", None, None);
        entry.sources.clear();
        let found = view.find(&entry, None).unwrap();
        assert_eq!((found.family, found.by), ("fam-a", FoundBy::Name));
    }

    #[test]
    fn a_folded_query_uses_the_provider_id_and_the_folded_version_label() {
        let mut lib = library();
        let slug_id = "owner-package";
        lib.families.insert(
            slug_id.into(),
            Family {
                id: slug_id.into(),
                name: "Not the slug".into(),
                sources: BTreeMap::from([(
                    ProviderId::new(PROVIDER),
                    source("Owner-Package", None),
                )]),
                ..Family::default()
            },
        );
        let mut version = mod_version(slug_id, slug_id, "A release", Some("1.2.3"));
        version.info.version = Some("1.2.3".into());
        lib.versions.insert(slug_id.into(), version);

        let view = LibraryView::new(&lib);
        let provider = ProviderId::new(PROVIDER);
        let exact = view
            .find_with(FindQuery::folded(&provider, "OWNER-package", "1.2.3"), None)
            .unwrap();
        assert_eq!(exact.family, slug_id);
        assert_eq!(
            exact.version.map(|version| version.folder.as_str()),
            Some(slug_id)
        );

        let other = view
            .find_with(FindQuery::folded(&provider, "owner-PACKAGE", "9.9.9"), None)
            .unwrap();
        assert_eq!(other.family, slug_id);
        assert!(other.version.is_none());
    }

    #[test]
    fn a_keyed_query_uses_the_first_folder_whose_key_matches() {
        let mut lib = library();
        lib.versions.insert(
            "mod-a".into(),
            mod_version("mod-a", "fam-a", "A release", Some("1.0")),
        );
        lib.versions.insert(
            "mod.a".into(),
            mod_version("mod.a", "fam-a", "A release", Some("2.0")),
        );
        let view = LibraryView::new(&lib);
        let found = view.find_with(FindQuery::by_key("Mod A!"), None).unwrap();
        assert_eq!(
            found.version.map(|version| version.folder.as_str()),
            Some("mod-a")
        );
    }
}
