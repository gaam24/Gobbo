//! The data read from a mod's page and the settings views built on it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::id::DownloadRoute;

/// The common counters a provider exposes for a page. A field the provider does not have stays `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceStats {
    pub downloads: Option<u32>,
    pub unique_downloads: Option<u32>,
    pub endorsements: Option<u32>,
    pub subscribers: Option<u32>,
}

/// Data from a mod's page on a provider, mapped to the fields the interface and the update check share.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct SourceMeta {
    pub name: Option<String>,
    pub author: Option<String>,
    pub uploader: Option<String>,
    pub category: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub picture_url: Option<String>,
    /// The copy of the picture in the game's `images/` folder, a file name.
    pub picture: Option<String>,
    pub version: Option<String>,
    pub stats: SourceStats,
    pub adult: bool,
    pub checked_at: Option<u32>,
    pub updated_at: Option<u32>,
    pub created_at: Option<u32>,
    pub status: Option<String>,
    pub tags: Vec<String>,
    /// Provider-specific fields that the shared model does not name. Values stay strings so they can cross IPC.
    pub extra: BTreeMap<String, String>,
}

impl SourceMeta {
    /// Keeps the saved picture when the page still names the same address. A new address keeps no copy, so the next
    /// read saves the new picture.
    pub fn keep_picture_from(&mut self, old: Option<&Self>) {
        let Some(old) = old else {
            return;
        };
        if self.picture_url.is_some() && self.picture_url == old.picture_url {
            self.picture.clone_from(&old.picture);
        }
    }

    /// Merges a fresh page read into the meta saved for the same mod. A picture this read did not save keeps the saved
    /// copy when the address is the same. A summary or date the read does not bring keeps the saved one, and the extra
    /// fields of both are kept, the fresh value winning a key both name. The check time is the caller's to set.
    pub fn merge_saved(&mut self, old: Option<&Self>) {
        let Some(old) = old else {
            return;
        };
        if self.picture.is_none() {
            self.keep_picture_from(Some(old));
        }
        if self.summary.is_none() {
            self.summary.clone_from(&old.summary);
        }
        if self.updated_at.is_none() {
            self.updated_at = old.updated_at;
        }
        if self.created_at.is_none() {
            self.created_at = old.created_at;
        }
        let mut extra = old.extra.clone();
        extra.append(&mut self.extra);
        self.extra = extra;
    }
}

/// A provider as the settings show it: its name, whether it keeps a key, whether the key has a page to open, and the
/// schemes of its download links (empty when it has none).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceProviderInfo {
    pub id: String,
    pub name: String,
    pub needs_key: bool,
    pub has_key_page: bool,
    /// The provider can download its files without a browser step. A saved account may still need a paid tier, which
    /// [`SourceAccount::direct_download`] reports for the account itself.
    pub direct_download: bool,
    /// Whether the saved account (or the provider, when it needs no key) downloads directly right now. This is the one
    /// answer every update and download button reads.
    pub download_route: DownloadRoute,
    /// The provider can install collections.
    pub collections: bool,
    pub schemes: Vec<String>,
}

/// Whether the system starts Gobbo for one of a provider's download-link schemes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceLinkStatus {
    /// The provider whose link scheme this is, by id.
    pub provider: String,
    /// The scheme without `://`, for example `nxm`.
    pub scheme: String,
    /// Whether this copy of Gobbo is the handler the system starts for the scheme.
    pub registered: bool,
}

/// What a check of the download links found, and which providers Gobbo registered again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceLinksReport {
    /// The handler the system has for each scheme the settings take.
    pub statuses: Vec<SourceLinkStatus>,
    /// The providers whose links Gobbo registered again during this check, by id.
    pub repaired: Vec<String>,
}

/// The account behind a key, as the provider describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceAccount {
    pub name: String,
    pub premium: bool,
    /// Whether this account can fetch a provider file without a browser step.
    pub direct_download: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_read_keeps_the_saved_values_it_does_not_bring() {
        let saved = SourceMeta {
            picture_url: Some("https://example.invalid/a.png".to_string()),
            picture: Some("provider-1.png".to_string()),
            summary: Some("Saved summary".to_string()),
            updated_at: Some(10),
            created_at: Some(5),
            extra: BTreeMap::from([
                ("kept".to_string(), "old".to_string()),
                ("shared".to_string(), "old".to_string()),
            ]),
            ..SourceMeta::default()
        };
        // The picture save failed: the same address, no new file.
        let mut fresh = SourceMeta {
            picture_url: Some("https://example.invalid/a.png".to_string()),
            updated_at: Some(20),
            extra: BTreeMap::from([("shared".to_string(), "new".to_string())]),
            ..SourceMeta::default()
        };
        fresh.merge_saved(Some(&saved));
        assert_eq!(fresh.picture.as_deref(), Some("provider-1.png"));
        assert_eq!(fresh.summary.as_deref(), Some("Saved summary"));
        assert_eq!(fresh.updated_at, Some(20), "the fresh date wins");
        assert_eq!(fresh.created_at, Some(5));
        assert_eq!(fresh.extra.get("kept").map(String::as_str), Some("old"));
        assert_eq!(fresh.extra.get("shared").map(String::as_str), Some("new"));
    }

    #[test]
    fn a_new_picture_address_keeps_no_saved_copy_and_a_new_copy_wins() {
        let saved = SourceMeta {
            picture_url: Some("https://example.invalid/a.png".to_string()),
            picture: Some("provider-1.png".to_string()),
            ..SourceMeta::default()
        };
        let mut moved = SourceMeta {
            picture_url: Some("https://example.invalid/b.png".to_string()),
            ..SourceMeta::default()
        };
        moved.merge_saved(Some(&saved));
        assert_eq!(moved.picture, None);

        let mut saved_again = SourceMeta {
            picture_url: Some("https://example.invalid/a.png".to_string()),
            picture: Some("provider-2.png".to_string()),
            ..SourceMeta::default()
        };
        saved_again.merge_saved(Some(&saved));
        assert_eq!(saved_again.picture.as_deref(), Some("provider-2.png"));
    }

    #[test]
    fn a_merge_without_a_saved_meta_keeps_the_fresh_read() {
        let mut fresh = SourceMeta {
            summary: Some("Fresh".to_string()),
            ..SourceMeta::default()
        };
        let before = fresh.clone();
        fresh.merge_saved(None);
        assert_eq!(fresh, before);
    }
}
