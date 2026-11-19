//! The Nexus API key: where it is kept (the OS credential store, never the settings file), the check that Nexus makes of
//! it, and the start of every request that needs it.

use super::api::NexusClient;
use crate::error::AppError;

/// The keyring account this provider stores its key under.
pub const ACCOUNT: &str = "nexus_api_key";

/// The saved key. Without one, the shared `source.key_missing` error asks the user for it.
pub fn required() -> Result<String, AppError> {
    crate::providers::required_key(&super::NexusProvider)
}

/// The name and account level of the API key's owner, shown as a hint in the settings. This is the only v1 request:
/// v3 has no user endpoint, and GraphQL cannot give the name and the premium flag in one query.
pub fn validate_key(api_key: &str) -> Result<(String, bool), AppError> {
    let info = NexusClient::validate_key(api_key)?;
    // Downloads read the tier from this cache; validating the key here keeps them from making another API call.
    super::remember_tier(api_key, info.premium);
    Ok((info.name, info.premium))
}
