//! Nexus collections: the public revision metadata, the archive link and the one URL a free account visits. The
//! archive itself is read by the `collections` feature; this module only maps the API to provider-neutral data.

use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::providers::link::is_slug;
use crate::providers::nexus::ID;
use crate::providers::{CollectionLink, CollectionRevision, CollectionSource};

mod download;
mod link;
mod revision;
mod types;

pub use link::parse_collection_link;

use download::download_archive;
use revision::revision_from_api;

/// The Nexus collection adapter.
pub struct NexusCollections;

impl CollectionSource for NexusCollections {
    fn parse_link(&self, url: &str) -> Result<CollectionLink, AppError> {
        parse_collection_link(url)
    }

    fn revision(
        &self,
        key: &str,
        link: &CollectionLink,
        revision: Option<u32>,
    ) -> Result<CollectionRevision, AppError> {
        revision_from_api(key, link, revision)
    }

    fn download_archive(
        &self,
        key: &str,
        game_key: &str,
        archive_link: &str,
        directory: &Path,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<PathBuf, AppError> {
        download_archive(key, game_key, archive_link, directory, progress)
    }

    fn page_url(&self, game_key: &str, slug: &str) -> Option<String> {
        if !is_slug(game_key, false) || !is_slug(slug, true) {
            return None;
        }

        Some(format!(
            "https://www.nexusmods.com/games/{game_key}/collections/{slug}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A live check against the real collection from the task. It needs the network and a temporary free key in
    /// `GOBBO_NEXUS_KEY`; it is ignored so the normal suite never depends on either.
    #[test]
    #[ignore = "needs GOBBO_NEXUS_KEY and the network"]
    fn a_real_collection_reads_and_downloads() {
        let Ok(key) = std::env::var("GOBBO_NEXUS_KEY") else {
            return;
        };
        let link = parse_collection_link(
            "https://www.nexusmods.com/games/cyberpunk2077/collections/n0nymh",
        )
        .unwrap();
        let revision = NexusCollections.revision(&key, &link, None).unwrap();
        assert_eq!(revision.revision, 49);
        assert_eq!(revision.mods.len(), 11);
        let archive_link = revision.archive_link.as_deref().unwrap();
        let dir = crate::test_support::scratch!("live-collection");
        let archive = NexusCollections
            .download_archive(&key, "cyberpunk2077", archive_link, &dir, &mut |_, _| true)
            .unwrap();
        let plan =
            crate::collections::manifest::read_plan(&archive, &revision, "cyberpunk2077").unwrap();
        assert_eq!(plan.items.len(), 11);
        assert_eq!(plan.order.len(), 11);
        assert!(!plan.limited);
        let _ = std::fs::remove_dir_all(dir);
    }
}
