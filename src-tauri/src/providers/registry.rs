//! The registered provider adapters and the lookup by id.

use super::port::SourceProvider;
use crate::error::AppError;

static NEXUS: super::nexus::NexusProvider = super::nexus::NexusProvider;
static THUNDERSTORE: super::thunderstore::ThunderstoreProvider =
    super::thunderstore::ThunderstoreProvider;

pub(super) static PROVIDERS: &[&dyn SourceProvider] = &[&NEXUS, &THUNDERSTORE];

/// Every provider this build knows, in the order the interface lists them.
pub fn providers() -> &'static [&'static dyn SourceProvider] {
    PROVIDERS
}

/// The provider override a test installed for `id`, when there is one. This item is compiled only under `cfg(test)`;
/// a shipped build has no such thing and [`provider`] resolves exactly as it always did.
#[cfg(test)]
fn test_override(id: &str) -> Option<&'static dyn SourceProvider> {
    super::testing::get(id)
}

/// The production half of the test seam: no override exists outside `cfg(test)`, so [`provider`] keeps its original
/// behaviour in every shipped build.
#[cfg(not(test))]
fn test_override(_id: &str) -> Option<&'static dyn SourceProvider> {
    None
}

/// The provider with this id, when it is registered. A test may install a network-free override for one id on its own
/// thread (the test-only `testing` module); with no override, this is the first registered provider with the id, exactly
/// as before.
pub fn provider(id: &str) -> Option<&'static dyn SourceProvider> {
    test_override(id).or_else(|| {
        PROVIDERS
            .iter()
            .copied()
            .find(|provider| provider.id().as_str() == id)
    })
}

/// The provider with this id. An unknown one is a user-facing error.
pub fn require(id: &str) -> Result<&'static dyn SourceProvider, AppError> {
    provider(id).ok_or_else(|| AppError::user("source.unknown").with("provider", id))
}
