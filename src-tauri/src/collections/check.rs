//! Refreshing the newest revision of the collections installed in a profile. The feature knows no provider names:
//! the adapter behind `CollectionSource` reads the current revision and the profile keeps the answer.

use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::context::{Data, load_profiles};
use crate::domain::collection::CollectionOrigin;
use crate::error::AppError;
use crate::infra::log;
use crate::providers;

/// Reads the current revision of every collection in `profile_id` and records what each provider reports. A collection
/// that cannot be read keeps its last answer; the others are still checked and saved in one profile edit.
pub fn check(root: &Data, game_id: &str, profile_id: &str) -> Result<(), AppError> {
    let data = root.game(game_id);
    // Provider reads can be slow, so they run without the profile lock. The profile is read again under the lock before
    // the write, so a list edit made while the providers answered is not overwritten.
    let origins = load_profiles(&data)?.get(profile_id)?.collections.clone();
    let mut answers = Vec::with_capacity(origins.len());
    for origin in &origins {
        match read_latest(origin) {
            Ok(latest) => answers.push((origin.key(), latest)),
            Err(error) => log::warn(format!(
                "collection check of {} failed: {}",
                origin.key(),
                error.code
            )),
        }
    }
    let _guard = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let changed = record_latest(&mut profiles.get_mut(profile_id)?.collections, &answers);
    if changed {
        save_profiles(&data, &profiles)?;
    }
    Ok(())
}

/// The provider read. The installed revision is passed so an adapter that can reuse it does not have to read the whole
/// newest recipe; the newest number comes from the provider's collection metadata either way.
fn read_latest(origin: &CollectionOrigin) -> Result<u32, AppError> {
    let collection = providers::collection_provider(&origin.provider)
        .ok_or_else(|| providers::unsupported_by(&origin.provider))?;
    let link = collection
        .parse_link(&origin.url)
        .map_err(|_| AppError::user("collection.bad_link").with("url", &origin.url))?;
    let source = providers::require(&origin.provider)?;
    let key = providers::saved_key(source)?.unwrap_or_default();
    Ok(collection
        .revision(&key, &link, Some(origin.revision))?
        .latest_revision)
}

/// Applies the answers to the origins of one profile. Only `latest_revision` changes, so a check can never disturb the
/// installed revision, the exclusions or the records of what the collection added.
fn record_latest(origins: &mut [CollectionOrigin], answers: &[(String, u32)]) -> bool {
    let mut changed = false;
    for (key, latest) in answers {
        if let Some(origin) = origins.iter_mut().find(|origin| &origin.key() == key)
            && origin.latest_revision != *latest
        {
            origin.latest_revision = *latest;
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(slug: &str, revision: u32, latest: u32) -> CollectionOrigin {
        CollectionOrigin {
            provider: "example".into(),
            game_key: "examplegame".into(),
            slug: slug.into(),
            revision,
            latest_revision: latest,
            revision_label: String::new(),
            name: "Example".into(),
            author: None,
            url: String::new(),
            excluded: Vec::new(),
            manual: Vec::new(),
            added: Vec::new(),
            families: Vec::new(),
            disabled: Vec::new(),
            installed_at: 0,
        }
    }

    #[test]
    fn an_answer_changes_only_the_newest_revision() {
        let mut origins = vec![origin("first", 2, 2), origin("second", 3, 3)];
        let answers = [("example:examplegame:first".to_string(), 5)];
        assert!(record_latest(&mut origins, &answers));
        assert_eq!(origins[0].latest_revision, 5);
        assert_eq!(origins[0].revision, 2);
        assert_eq!(origins[1].latest_revision, 3);
        assert!(!record_latest(&mut origins, &answers));
    }

    #[test]
    fn an_answer_for_an_absent_collection_is_ignored() {
        let mut origins = vec![origin("first", 2, 2)];
        let answers = [("example:examplegame:gone".to_string(), 9)];
        assert!(!record_latest(&mut origins, &answers));
        assert_eq!(origins[0].latest_revision, 2);
    }
}
