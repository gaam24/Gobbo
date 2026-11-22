//! The Nexus update engine: page data, the newest file and the requirements that Nexus lists for it. The feature in
//! `sources::update` builds generic requests and stores the generic results; no library or profile type enters here.

mod lookup;
mod requirements;
mod workers;

use crate::domain::source::{SourceMeta, SourceRequirement, SourceSync};
use crate::providers::nexus::api::NexusClient;
use crate::providers::nexus::{ModPage, meta_from_page};
use crate::providers::{SourceUpdate, SourceUpdateRequest};

/// The requirements Nexus lists for the newest file of a family, as the family keeps them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Listed {
    pub(super) requires: Vec<crate::providers::nexus::NexusRequirement>,
    pub(super) dlcs: Vec<Vec<String>>,
    /// Where the list came from: `None` when no file of the mod is known to Nexus.
    pub(super) sync: Option<crate::providers::nexus::NexusRequiresSync>,
}

pub(super) fn check_updates(api_key: &str, requests: &[SourceUpdateRequest]) -> Vec<SourceUpdate> {
    let domain = requests.first().map(|request| request.game_key.as_str());
    let client = NexusClient::new(api_key);
    let mut results = Vec::new();
    let mut lookups = Vec::new();

    for request in requests {
        match lookup::lookup(request) {
            Ok(lookup) => lookups.push(lookup),
            Err(error) => results.push(SourceUpdate::error(request, error)),
        }
    }

    let Some(domain) = domain else {
        return results;
    };

    if !lookups.is_empty() {
        results.extend(workers::look_up_all(&client, domain, &lookups));
    }

    results
}

/// The page data merged with the old values. A field the page does not bring keeps the saved one; the picture file and
/// the check time are this run's.
pub(super) fn page_meta(
    old: Option<&SourceMeta>,
    page: &ModPage,
    checked_at: u32,
    picture: Option<String>,
) -> SourceMeta {
    let mut fresh = meta_from_page(page);
    fresh.picture = picture;
    SourceMeta::checked(fresh, old, checked_at)
}

/// The generic requirements a read brought, with the sync that says where they came from.
fn requirements(
    listed: Option<Listed>,
) -> (
    Option<Vec<SourceRequirement>>,
    Vec<Vec<String>>,
    Option<SourceSync>,
) {
    match listed {
        Some(listed) => (
            Some(
                listed
                    .requires
                    .into_iter()
                    .map(SourceRequirement::from)
                    .collect(),
            ),
            listed.dlcs,
            listed.sync.map(SourceSync::from),
        ),
        None => (None, Vec::new(), None),
    }
}

#[cfg(test)]
mod tests;
