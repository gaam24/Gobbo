//! Provider-neutral data for mods that come from an external source.
//!
//! The library, the profiles and the deployment plan store this data, but they never know which service it came
//! from. A provider adapter maps its own API to these types; the rest of the app works with `SourceRef` and
//! `SourceMeta` only.

mod id;
mod page;
mod reference;

pub use id::{DownloadRoute, ProviderId, SourceAuthKind, SourceCapabilities};
pub use page::{
    SourceAccount, SourceLinkStatus, SourceLinksReport, SourceMeta, SourceProviderInfo, SourceStats,
};
pub use reference::{SourceLink, SourceMod, SourceRef, SourceRequirement, SourceSync};
