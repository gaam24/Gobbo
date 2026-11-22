//! The source, requirement and update views of a mod: the small row data, the inspector data and the saved check.

use serde::Serialize;

use crate::domain::source::SourceMeta;
use crate::error::AppError;
use crate::library::relations::RequirementState;

/// One requirement of a mod, on any provider, with its state in the profile.
#[derive(Debug, Serialize, specta::Type)]
pub struct SourceRequirementView {
    pub source: crate::domain::source::SourceRef,
    pub name: String,
    pub page_url: Option<String>,
    pub state: RequirementState,
    /// The provider can fetch this requirement without leaving Gobbo, so the row can offer **Get**.
    pub direct_download: bool,
    pub from_source: bool,
    pub group: Option<u32>,
    pub ignored: bool,
}

/// The DLC notes one provider lists for a mod.
#[derive(Debug, Serialize, specta::Type)]
pub struct ProviderDlcs {
    pub provider: String,
    pub dlcs: Vec<Vec<String>>,
}

/// When a provider's requirements were last read.
#[derive(Debug, Serialize, specta::Type)]
pub struct SourceSyncView {
    pub provider: String,
    pub checked_at: u32,
    pub older_version: Option<String>,
}

/// The result of checking one source of a mod family: the latest version, or the reason there is none. The status carries
/// the result of the last saved check too, so the list shows it after a restart or a game switch.
#[derive(Debug, Hash, Serialize, specta::Type)]
pub struct UpdateReport {
    /// Id of the mod family.
    pub id: String,
    /// The provider that was checked.
    pub provider: String,
    /// The mod id on the provider.
    pub mod_id: String,
    /// The version the profile selects now.
    pub current: Option<String>,
    /// The newest version the provider listed at the check.
    pub latest: Option<String>,
    /// The provider's id of the newest file, when the check read a file chain. It names the exact file a manual update
    /// has to download, also when the page has several parts.
    pub latest_file_id: Option<String>,
    /// How `latest` compares with `current`, or that the user ignored it.
    pub state: crate::domain::version::UpdateState,
    /// The latest version is already installed in the library, just not selected in this profile.
    pub latest_installed: bool,
    /// The library folder of that installed latest version, when `latest_installed` is true. "Use installed version"
    /// selects this folder.
    pub installed_folder: Option<String>,
    /// Whether the newest version is an update the user can take (see `UpdateState::is_update`). The window reads this
    /// instead of the state.
    pub has_update: bool,
    /// Why the check failed. Only a check that ran in this session has one; a saved result never does.
    pub error: Option<AppError>,
}

impl UpdateReport {
    /// Fills what the library decides for the report: whether the latest version is installed and in which folder, and
    /// whether it is an update. A provider file id is exact, so it is preferred over the version string.
    pub fn with_library(
        mut self,
        view: &crate::domain::library::LibraryView<'_>,
        latest_file: Option<&str>,
    ) -> Self {
        self.installed_folder = view
            .installed_latest_folder(
                &self.id,
                &self.provider,
                latest_file,
                self.latest.as_deref(),
            )
            .map(str::to_string);
        self.latest_installed = self.installed_folder.is_some();
        self.has_update = self.state.is_update();
        self
    }
}

/// The page data a list row needs: who made the mod and which picture and category it shows.
#[derive(Debug, Hash, Serialize, specta::Type)]
pub struct SourcePreview {
    pub provider: String,
    pub mod_id: String,
    pub name: Option<String>,
    pub author: Option<String>,
    pub uploader: Option<String>,
    pub category: Option<String>,
    pub picture: Option<String>,
    pub picture_url: Option<String>,
    pub adult: bool,
    /// When the page was last checked. The version cell shows it on hover.
    pub checked_at: Option<u32>,
}

impl SourcePreview {
    /// The preview fields of one source from its page data. A source without page data still gets its provider and id.
    pub fn from_meta(provider: &str, mod_id: &str, meta: Option<&SourceMeta>) -> Self {
        // Clone only the fields the row shows. Cloning the whole `SourceMeta` would copy the full description, tags
        // and provider extras too, once for every row of a large library.
        let empty = SourceMeta::default();
        let meta = meta.unwrap_or(&empty);
        Self {
            provider: provider.to_string(),
            mod_id: mod_id.to_string(),
            name: meta.name.clone(),
            author: meta.author.clone(),
            uploader: meta.uploader.clone(),
            category: meta.category.clone(),
            picture: meta.picture.clone(),
            picture_url: meta.picture_url.clone(),
            adult: meta.adult,
            checked_at: meta.checked_at,
        }
    }
}

/// The full page fields of one source, for the inspector and the columns that read them. The description is a preview;
/// the full text stays in the details view.
#[derive(Debug, Serialize, specta::Type)]
pub struct SourceSummary {
    pub provider: String,
    pub mod_id: String,
    pub name: Option<String>,
    pub author: Option<String>,
    pub uploader: Option<String>,
    pub category: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub picture: Option<String>,
    pub picture_url: Option<String>,
    pub downloads: Option<u32>,
    pub unique_downloads: Option<u32>,
    pub endorsements: Option<u32>,
    pub adult: bool,
    pub checked_at: Option<u32>,
    pub updated_at: Option<u32>,
    pub created_at: Option<u32>,
    pub status: Option<String>,
    pub tags: Vec<String>,
}

impl SourceSummary {
    /// The summary of one source from its page data. A source without page data still gets its provider and mod id.
    pub fn from_meta(provider: &str, mod_id: &str, meta: Option<&SourceMeta>) -> Self {
        // As in the preview, clone only the fields the view carries; the full description stays in the library.
        let empty = SourceMeta::default();
        let meta = meta.unwrap_or(&empty);
        Self {
            provider: provider.to_string(),
            mod_id: mod_id.to_string(),
            name: meta.name.clone(),
            author: meta.author.clone(),
            uploader: meta.uploader.clone(),
            category: meta.category.clone(),
            summary: meta.summary.clone(),
            description: meta.description.as_deref().map(description_preview),
            picture: meta.picture.clone(),
            picture_url: meta.picture_url.clone(),
            downloads: meta.stats.downloads,
            unique_downloads: meta.stats.unique_downloads,
            endorsements: meta.stats.endorsements,
            adult: meta.adult,
            checked_at: meta.checked_at,
            updated_at: meta.updated_at,
            created_at: meta.created_at,
            status: meta.status.clone(),
            tags: meta.tags.clone(),
        }
    }
}

/// How much of a description the status list carries. The inspector reads the full text from `mod_details`.
const DESCRIPTION_PREVIEW_CHARS: usize = 512;

/// A page description shortened to the list preview, at a character boundary.
pub(in crate::status) fn description_preview(text: &str) -> String {
    // Almost every page text is ASCII. The byte cut is the character cut there, so the scanner is only needed for
    // the rare text that can split a character.
    if text.is_ascii() {
        if text.len() <= DESCRIPTION_PREVIEW_CHARS {
            return text.to_string();
        }

        let mut preview = String::with_capacity(DESCRIPTION_PREVIEW_CHARS + '…'.len_utf8());
        preview.push_str(&text[..DESCRIPTION_PREVIEW_CHARS]);
        preview.push('…');
        return preview;
    }

    match text.char_indices().nth(DESCRIPTION_PREVIEW_CHARS) {
        Some((cut, _)) => {
            let mut preview = String::with_capacity(cut + '…'.len_utf8());
            preview.push_str(&text[..cut]);
            preview.push('…');
            preview
        }
        None => text.to_string(),
    }
}

/// One external source of a mod in the list. It stays small: the page data belongs to the details view.
#[derive(Debug, Hash, Serialize, specta::Type)]
pub struct ModSource {
    /// The provider id, for example `nexus`.
    pub provider: String,
    pub mod_id: String,
    /// The page of the mod, when the provider has one and the game has a key for it.
    pub page_url: Option<String>,
    /// The file the selected version came from, when the provider named one.
    pub file_id: Option<String>,
    /// The persistent part the selected version belongs to.
    pub part_id: Option<String>,
    pub part_name: Option<String>,
    pub part_category: Option<String>,
}

/// One external source with its full page data, for the inspector. The list never carries this much text.
#[derive(Debug, Serialize, specta::Type)]
pub struct ModSourceDetails {
    pub provider: String,
    pub mod_id: String,
    pub page_url: Option<String>,
    pub file_id: Option<String>,
    pub part_id: Option<String>,
    pub part_name: Option<String>,
    pub part_category: Option<String>,
    pub meta: Option<SourceMeta>,
}

impl From<ModSource> for ModSourceDetails {
    fn from(source: ModSource) -> Self {
        Self {
            provider: source.provider,
            mod_id: source.mod_id,
            page_url: source.page_url,
            file_id: source.file_id,
            part_id: source.part_id,
            part_name: source.part_name,
            part_category: source.part_category,
            meta: None,
        }
    }
}
