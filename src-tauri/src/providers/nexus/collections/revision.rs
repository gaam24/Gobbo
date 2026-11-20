use serde_json::json;

use crate::domain::collection::CollectionInfo;
use crate::error::AppError;
use crate::providers::nexus::api::{NexusClient, bad_response};
use crate::providers::{
    CollectionExternal, CollectionLink, CollectionMod, CollectionRevision,
    CollectionRevisionSummary,
};

use super::types::{COLLECTION_QUERY, GraphQlResponse, Id};

pub(super) fn revision_from_api(
    key: &str,
    link: &CollectionLink,
    revision: Option<u32>,
) -> Result<CollectionRevision, AppError> {
    let client = if key.trim().is_empty() {
        NexusClient::anonymous()
    } else {
        NexusClient::new(key)
    };
    let body = json!({
        "query": COLLECTION_QUERY,
        "variables": {
            "slug": link.slug,
            "domainName": link.game_key,
            "revision": revision,
            "adult": true,
        },
    })
    .to_string();
    let text = client.post_graphql(&body)?;
    parse_revision_response(&text, link, revision)
}

/// A missing collection is the collection `kind` of the one Nexus 404 code. The slug is kept for detail.
fn collection_not_found(slug: &str) -> AppError {
    AppError::user(crate::error::NEXUS_NOT_FOUND)
        .with("kind", "collection")
        .with("slug", slug)
}

fn parse_revision_response(
    text: &str,
    link: &CollectionLink,
    requested: Option<u32>,
) -> Result<CollectionRevision, AppError> {
    let response: GraphQlResponse = serde_json::from_str(text).map_err(bad_response)?;
    let Some(data) = response.data else {
        let detail = response
            .errors
            .first()
            .and_then(|error| error.message.clone())
            .unwrap_or_else(|| "no data".to_string());
        if detail.contains("NOT_FOUND") || detail.contains("not found") {
            return Err(collection_not_found(&link.slug));
        }

        return Err(bad_response(detail));
    };
    let Some(collection) = data.collection else {
        return Err(collection_not_found(&link.slug));
    };
    let Some(revision) = data.collection_revision else {
        return Err(AppError::user("nexus.collection_revision_missing")
            .with("slug", &link.slug)
            .with("revision", requested.unwrap_or_default()));
    };
    let revision_number = revision
        .revision_number
        .and_then(Id::u32)
        .or(requested)
        .ok_or_else(|| bad_response("the revision has no number"))?;
    let latest = collection
        .latest_published_revision
        .clone()
        .unwrap_or_else(|| revision.clone());
    let latest_revision = latest
        .revision_number
        .and_then(Id::u32)
        .unwrap_or(revision_number);
    let mod_count = revision
        .mod_count
        .and_then(Id::u32)
        .unwrap_or_else(|| u32::try_from(revision.mod_files.len()).unwrap_or(u32::MAX));
    let total_size_kb = revision.total_size.map_or(0, Id::kb);
    let info = CollectionInfo {
        provider: "nexus".to_string(),
        game_key: link.game_key.clone(),
        slug: link.slug.clone(),
        revision: revision_number,
        latest_revision,
        revision_label: format!("r{revision_number}"),
        name: collection.name.clone().unwrap_or_else(|| link.slug.clone()),
        author: collection.user.and_then(|user| user.name),
        summary: collection.summary.unwrap_or_default(),
        description: collection.description.unwrap_or_default(),
        instructions: revision
            .installation_info
            .clone()
            .or_else(|| latest.installation_info.clone())
            .unwrap_or_default(),
        picture_url: collection.tile_image.and_then(|image| image.thumbnail_url),
        adult: collection.adult_content.unwrap_or(false) || revision.adult_content.unwrap_or(false),
        total_size_kb,
        mod_count,
        game_versions: revision
            .game_versions
            .iter()
            .chain(latest.game_versions.iter())
            .filter_map(|version| version.reference.clone())
            .collect(),
        changelog: revision
            .collection_changelog
            .as_ref()
            .and_then(|changelog| changelog.description.clone())
            .or_else(|| {
                latest
                    .collection_changelog
                    .as_ref()
                    .and_then(|changelog| changelog.description.clone())
            }),
        url: format!(
            "https://www.nexusmods.com/games/{}/collections/{}",
            link.game_key, link.slug
        ),
    };
    let mut game_versions = info.game_versions.clone();
    game_versions.sort();
    game_versions.dedup();
    let mut info = info;
    info.game_versions = game_versions;

    let mut mods = Vec::with_capacity(revision.mod_files.len());

    for entry in revision.mod_files {
        let (Some(mod_id), Some(file_id), Some(file)) = (
            entry
                .file
                .as_ref()
                .and_then(|file| file.r#mod.as_ref())
                .and_then(|module| module.mod_id)
                .and_then(Id::u32),
            entry.file_id.and_then(Id::u32),
            entry.file,
        ) else {
            continue;
        };
        let module = file.r#mod.unwrap_or_default();
        let size_kb = file
            .size_in_bytes
            .map_or_else(|| file.size.map_or(0, Id::kb), Id::kb);
        mods.push(CollectionMod {
            mod_id: mod_id.to_string(),
            file_id: file_id.to_string(),
            optional: entry.optional,
            name: module.name.unwrap_or_else(|| format!("Nexus {mod_id}")),
            version: file.version,
            author: module.author,
            category: module.category,
            summary: module.summary.unwrap_or_default(),
            picture_url: module.picture_url,
            adult: module.adult,
            file_name: file.name,
            size_kb,
        });
    }

    let external = revision
        .external_resources
        .into_iter()
        .map(|resource| CollectionExternal {
            name: resource.name.unwrap_or_default(),
            resource_type: resource.resource_type.unwrap_or_default(),
            url: resource.resource_url,
        })
        .collect();
    let mut revisions = Vec::new();

    for revision in collection.revisions {
        let Some(number) = revision.revision_number.and_then(Id::u32) else {
            continue;
        };

        if revision
            .revision_status
            .as_deref()
            .is_some_and(|status| status == "discarded")
            || revision.discarded_at.is_some()
        {
            continue;
        }

        revisions.push(CollectionRevisionSummary {
            revision: number,
            created_at: revision
                .created_at
                .as_deref()
                .and_then(crate::infra::clock::unix_time),
            latest: revision.latest.unwrap_or(number == latest_revision),
            mod_count: revision.mod_count.and_then(Id::u32).unwrap_or(0),
            total_size_kb: revision.total_size.map_or(0, Id::kb),
        });
    }

    revisions.sort_by_key(|revision| revision.revision);
    revisions.dedup_by_key(|revision| revision.revision);
    Ok(CollectionRevision {
        revision: revision_number,
        latest_revision,
        info,
        mods,
        external,
        revisions,
        archive_link: revision.download_link,
        complete: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::source::ProviderId;

    #[test]
    fn the_revision_list_skips_discarded_revisions() {
        let text = r#"{"data":{
          "collection":{
            "name":"Example","summary":"S","description":"D","adultContent":false,
            "user":{"name":"Author"},"tileImage":{"thumbnailUrl":"https://example.invalid/x.png"},
            "latestPublishedRevision":{"revisionNumber":2,"installationInfo":null,
              "collectionChangelog":{"description":"two"},"gameVersions":[{"reference":"1.0"}]},
            "revisions":[
              {"revisionNumber":2,"createdAt":"2026-01-02T00:00:00Z","discardedAt":null,"latest":true,"revisionStatus":"published","totalSize":"2048","modCount":1},
              {"revisionNumber":1,"createdAt":"2026-01-01T00:00:00Z","discardedAt":"2026-01-03T00:00:00Z","latest":false,"revisionStatus":"discarded","totalSize":"2048","modCount":1}
            ]
          },
          "collectionRevision":{
            "revisionNumber":2,"downloadLink":"/v2/collections/1/revisions/2/download_link",
            "installationInfo":null,"adultContent":false,"totalSize":"2048","modCount":1,
            "collectionChangelog":{"description":"two"},"gameVersions":[{"reference":"1.0"}],
            "externalResources":[],
            "modFiles":[{"optional":false,"fileId":10,"file":{"name":"file.zip","version":"1.0","size":2,"sizeInBytes":2048,"mod":{"modId":7,"name":"Mod","author":"A","category":"C","adult":false,"pictureUrl":null,"summary":"S"}}}]
          }
        }}"#;
        let link = CollectionLink {
            provider: ProviderId::new("nexus"),
            game_key: "examplegame".to_string(),
            slug: "example".to_string(),
            revision: None,
            page_url: None,
        };
        let revision = parse_revision_response(text, &link, None).unwrap();
        assert_eq!(revision.revision, 2);
        assert_eq!(revision.latest_revision, 2);
        assert_eq!(revision.mods.len(), 1);
        assert_eq!(revision.mods[0].mod_id, "7");
        assert_eq!(revision.mods[0].file_id, "10");
        assert_eq!(revision.revisions.len(), 1);
        assert_eq!(revision.info.total_size_kb, 2);
        assert_eq!(revision.info.changelog.as_deref(), Some("two"));
    }
}
