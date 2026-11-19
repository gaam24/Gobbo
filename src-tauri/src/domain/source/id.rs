//! The provider id and the auth and capability model that describe a service without naming it.

use std::fmt;

use serde::{Deserialize, Serialize};
use specta::Type;

/// The id of a mod platform, for example `nexus` or `modio`. The list is open: a provider chooses its own id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    /// A provider id from text. The caller is responsible for using the provider's canonical spelling.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The id as the provider spells it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for ProviderId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ProviderId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl AsRef<str> for ProviderId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for ProviderId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// How a provider authenticates a user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SourceAuthKind {
    #[default]
    None,
    ApiKey,
    OAuth,
}

/// What a provider can do. The interface and the update checks ask these flags instead of the provider's name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceCapabilities {
    pub update_check: bool,
    pub direct_download: bool,
    pub requirements: bool,
    pub collections: bool,
    pub deep_link: bool,
    pub hash_match: bool,
    pub auth: SourceAuthKind,
}

impl SourceCapabilities {
    /// The capabilities of a provider that checks updates, downloads directly, reads requirements and deep links,
    /// with the given authentication.
    pub const fn new(auth: SourceAuthKind) -> Self {
        Self {
            update_check: true,
            direct_download: true,
            requirements: true,
            collections: false,
            deep_link: true,
            hash_match: false,
            auth,
        }
    }
}

/// Where a provider's file is downloaded: by Gobbo itself, or by the user in the provider's web page. The backend decides
/// it once (`providers::download_route`); the window only reads the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DownloadRoute {
    Direct,
    Browser,
}
