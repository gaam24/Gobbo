//! The sources of a mod: the row preview, the list entries and the inspector details.

use crate::games::GameDef;
use crate::library::{Library, ModVersion};
use crate::providers;
use crate::status::views::{ModSource, ModSourceDetails, SourcePreview, SourceSummary};

/// The first source with page data, or the first source at all. The row, its preview and the full extras use it.
pub(in crate::status) fn page_source(
    family: Option<&crate::library::Family>,
) -> Option<(
    &crate::domain::source::ProviderId,
    &crate::domain::source::SourceMod,
)> {
    let family = family?;
    family
        .sources
        .iter()
        .find(|(_, source)| source.meta.is_some())
        .or_else(|| family.sources.iter().next())
}

/// The page fields the list row draws: provider, names, picture, category and the adult flag.
pub(in crate::status) fn source_preview(
    family: Option<&crate::library::Family>,
) -> Option<SourcePreview> {
    let (provider, source) = page_source(family)?;
    Some(source_preview_of(provider, source))
}

/// The preview fields of one source, from the provider and its page data.
fn source_preview_of(
    provider: &crate::domain::source::ProviderId,
    source: &crate::domain::source::SourceMod,
) -> SourcePreview {
    SourcePreview::from_meta(provider.as_str(), &source.mod_id, source.meta.as_ref())
}

/// A source before the caller decides whether it needs the page data. Borrowing the metadata keeps the list path free
/// of clones.
struct SourceBase<'a> {
    provider: &'a str,
    mod_id: &'a str,
    page_url: Option<String>,
    file_id: Option<String>,
    part_id: Option<String>,
    part_name: Option<String>,
    part_category: Option<String>,
    meta: Option<&'a crate::domain::source::SourceMeta>,
}

impl SourceBase<'_> {
    /// The list view of the source, without the page data.
    fn mod_source(&self) -> ModSource {
        ModSource {
            provider: self.provider.to_string(),
            mod_id: self.mod_id.to_string(),
            page_url: self.page_url.clone(),
            file_id: self.file_id.clone(),
            part_id: self.part_id.clone(),
            part_name: self.part_name.clone(),
            part_category: self.part_category.clone(),
        }
    }

    /// The inspector view of the source, with the page data.
    fn mod_source_details(&self) -> ModSourceDetails {
        let mut details: ModSourceDetails = self.mod_source().into();
        details.meta = self.meta.cloned();
        details
    }
}

fn source_bases<'a>(
    family: Option<&'a crate::library::Family>,
    game: &'a GameDef,
    library: &'a Library,
    version: Option<&'a ModVersion>,
) -> impl Iterator<Item = SourceBase<'a>> + 'a {
    let archive = version
        .and_then(|version| version.archive.as_ref())
        .and_then(|sha256| library.archives.get(sha256));
    family.into_iter().flat_map(move |family| {
        family.sources.iter().map(move |(provider, source)| {
            let page_url = providers::page_url_for(
                provider.as_str(),
                game.provider_key(provider.as_str()),
                &source.mod_id,
                None,
            );
            SourceBase {
                provider: provider.as_str(),
                mod_id: &source.mod_id,
                page_url,
                file_id: archive
                    .and_then(|archive| archive.source_files.get(provider))
                    .cloned(),
                part_id: archive
                    .and_then(|archive| archive.source_parts.get(provider))
                    .cloned()
                    .or_else(|| source.part_id.clone())
                    .or_else(|| {
                        version.and_then(|version| version.parsed_parts.get(provider).cloned())
                    }),
                part_name: source.part_name.clone(),
                part_category: source.part_category.clone(),
                meta: source.meta.as_ref(),
            }
        })
    })
}

/// The first source with page data, flattened for the generic columns and the overview.
pub(in crate::status) fn source_summary(
    family: Option<&crate::library::Family>,
) -> Option<SourceSummary> {
    let (provider, source) = page_source(family)?;
    Some(SourceSummary::from_meta(
        provider.as_str(),
        &source.mod_id,
        source.meta.as_ref(),
    ))
}

/// The external sources of a family for the list, without the page text.
pub(in crate::status) fn source_views(
    family: Option<&crate::library::Family>,
    game: &GameDef,
    library: &Library,
    version: Option<&ModVersion>,
) -> Vec<ModSource> {
    source_bases(family, game, library, version)
        .map(|source| source.mod_source())
        .collect()
}

/// The external sources with their page data, for the inspector.
pub(in crate::status) fn source_details(
    family: Option<&crate::library::Family>,
    game: &GameDef,
    library: &Library,
    version: Option<&ModVersion>,
) -> Vec<ModSourceDetails> {
    source_bases(family, game, library, version)
        .map(|source| source.mod_source_details())
        .collect()
}

/// The page address of a requirement, when its provider has one and the game has a key.
pub(in crate::status) fn requirement_page_url(
    game: &GameDef,
    source: &crate::domain::source::SourceRef,
) -> Option<String> {
    providers::page_url_for(
        source.provider.as_str(),
        game.provider_key(source.provider.as_str()),
        &source.mod_id,
        None,
    )
}
