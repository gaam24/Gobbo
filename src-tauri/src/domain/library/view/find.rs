//! The one rule that answers "which mod of the library is this?". A pack mod, a waiting entry and a collection member
//! all describe the mod they want as a `PackMod`; every caller asks this rule instead of keeping its own index, so a
//! fix to the matching reaches packs, collections and the waiting list at once.

use std::collections::BTreeMap;

use super::LibraryView;
use crate::domain::library::ModVersion;
use crate::domain::pack::PackMod;

/// How the library mod was recognised, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundBy {
    /// The version came from the archive the mod names, by its SHA-256 sum.
    Archive,
    /// The version came from the exact provider file the mod names.
    File,
    /// The family is the provider page (and part) the mod names; the file may differ.
    Page,
    /// The family has the mod's name; nothing else identifies it.
    Name,
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
        if let Some(found) = self.by_archive(wanted, imported) {
            return Some(found);
        }
        // The provider's exact file id is stronger than the version label: a recipe can name a version differently from
        // the archive it points at.
        for (provider, source) in &wanted.sources {
            let Some(file_id) = wanted.source_files.get(provider) else {
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
            .family_of_wanted_page(wanted)
            .map(|family| (family, FoundBy::Page))
            .or_else(|| {
                self.family_named_loosely(&wanted.name)
                    .map(|family| (family, FoundBy::Name))
            })?;
        let version = wanted.version.as_deref().map_or_else(
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

    fn by_archive(
        &self,
        wanted: &PackMod,
        imported: Option<&BTreeMap<String, Vec<String>>>,
    ) -> Option<Found<'a>> {
        let sha256 = &wanted.archive.as_ref()?.sha256;
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
            .find(|version| version.info.name == wanted.name)
            .or_else(|| candidates.first())
            .copied()?;
        Some(Found {
            family: version.family.as_str(),
            version: Some(version),
            by: FoundBy::Archive,
        })
    }

    /// The family of the page and part `wanted` names. Without a part id, the part whose name the mod carries wins,
    /// then the page's first family, so two addons of one page do not resolve to the same family by accident.
    fn family_of_wanted_page(&self, wanted: &PackMod) -> Option<&'a str> {
        wanted
            .sources
            .iter()
            .find_map(|(provider, source)| {
                let part = source
                    .part_id
                    .as_deref()
                    .or_else(|| wanted.source_parts.get(provider).map(String::as_str))?;
                self.family_of_part(provider, &source.mod_id, part)
            })
            .or_else(|| {
                let name = wanted.part_name.as_deref().unwrap_or(&wanted.name).trim();
                wanted.sources.iter().find_map(|(provider, source)| {
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
}
