use serde::Deserialize;

pub(super) const COLLECTION_QUERY: &str = r"
query Collection($slug: String!, $domainName: String!, $revision: Int, $adult: Boolean!) {
  collection(slug: $slug, domainName: $domainName, viewAdultContent: $adult) {
    name summary description adultContent
    user { name }
    tileImage { thumbnailUrl(size: small) }
    latestPublishedRevision {
      revisionNumber installationInfo
      collectionChangelog { description }
      gameVersions { reference }
    }
    revisions {
      id revisionNumber createdAt discardedAt latest revisionStatus totalSize modCount
    }
  }
  collectionRevision(slug: $slug, domainName: $domainName, revision: $revision, viewAdultContent: $adult) {
    revisionNumber downloadLink installationInfo adultContent totalSize modCount
    collectionChangelog { description }
    gameVersions { reference }
    externalResources { name resourceType resourceUrl }
    modFiles {
      optional
      fileId
      file {
        name version size sizeInBytes
        mod { modId name author category adult pictureUrl summary }
      }
    }
  }
}
";

#[derive(Debug, Deserialize)]
pub(super) struct GraphQlResponse {
    pub(super) data: Option<GraphQlData>,
    #[serde(default)]
    pub(super) errors: Vec<GraphQlError>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GraphQlError {
    pub(super) message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GraphQlData {
    pub(super) collection: Option<CollectionNode>,
    pub(super) collection_revision: Option<RevisionNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CollectionNode {
    pub(super) name: Option<String>,
    pub(super) summary: Option<String>,
    pub(super) description: Option<String>,
    pub(super) adult_content: Option<bool>,
    pub(super) user: Option<UserNode>,
    pub(super) tile_image: Option<ImageNode>,
    pub(super) latest_published_revision: Option<RevisionNode>,
    #[serde(default)]
    pub(super) revisions: Vec<RevisionNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UserNode {
    pub(super) name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ImageNode {
    pub(super) thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RevisionNode {
    pub(super) revision_number: Option<Id>,
    pub(super) adult_content: Option<bool>,
    pub(super) mod_count: Option<Id>,
    pub(super) total_size: Option<Id>,
    pub(super) installation_info: Option<String>,
    pub(super) download_link: Option<String>,
    pub(super) created_at: Option<String>,
    pub(super) discarded_at: Option<String>,
    pub(super) latest: Option<bool>,
    pub(super) revision_status: Option<String>,
    pub(super) collection_changelog: Option<ChangelogNode>,
    #[serde(default)]
    pub(super) game_versions: Vec<GameVersionNode>,
    #[serde(default)]
    pub(super) external_resources: Vec<ExternalNode>,
    #[serde(default)]
    pub(super) mod_files: Vec<ModFileNode>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChangelogNode {
    pub(super) description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct GameVersionNode {
    pub(super) reference: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExternalNode {
    pub(super) name: Option<String>,
    pub(super) resource_type: Option<String>,
    pub(super) resource_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModFileNode {
    #[serde(default)]
    pub(super) optional: bool,
    pub(super) file_id: Option<Id>,
    pub(super) file: Option<FileNode>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileNode {
    pub(super) name: Option<String>,
    pub(super) version: Option<String>,
    pub(super) size: Option<Id>,
    pub(super) size_in_bytes: Option<Id>,
    pub(super) r#mod: Option<ModNode>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModNode {
    pub(super) mod_id: Option<Id>,
    pub(super) name: Option<String>,
    pub(super) author: Option<String>,
    pub(super) category: Option<String>,
    #[serde(default)]
    pub(super) adult: bool,
    pub(super) picture_url: Option<String>,
    pub(super) summary: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Id(u64);

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Value {
            Number(u64),
            Text(String),
        }

        match Value::deserialize(deserializer)? {
            Value::Number(value) => Ok(Self(value)),
            Value::Text(text) => text.parse().map(Self).map_err(serde::de::Error::custom),
        }
    }
}

impl Id {
    pub(super) fn as_u32(self) -> Option<u32> {
        u32::try_from(self.0).ok()
    }

    pub(super) fn as_kb(self) -> u32 {
        let bytes = self.0;
        u32::try_from(bytes.saturating_div(1024)).unwrap_or(u32::MAX)
    }
}
