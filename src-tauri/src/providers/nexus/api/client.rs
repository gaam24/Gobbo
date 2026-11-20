//! One client for every Nexus request: the base addresses, the key header and the rate limit. The retry and backoff
//! policy lives in `infra::http::Client`, shared with every provider.

use super::error::{ApiError, Endpoint};
use crate::error::AppError;
use crate::infra::http::{self, Client};
use serde::Deserialize;

const BASE_V1: &str = "https://api.nexusmods.com/v1";
/// The signed archive links of collections.
const BASE_V2: &str = "https://api.nexusmods.com";
/// The current API: mods, file versions, download links and file dependencies.
const BASE_V3: &str = "https://api.nexusmods.com/v3";
/// The GraphQL API. It is used only for what v3 does not expose: a mod's summary, its dates and its own requirements.
const GRAPHQL: &str = "https://api.nexusmods.com/v2/graphql";

/// The name and account level of the API key's owner, shown as a hint in the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    pub name: String,
    pub premium: bool,
}

/// The Nexus API client for one operation. It owns the key and the rate limiter; sharing it across the update
/// workers shares both.
pub struct NexusClient {
    client: Client,
}

impl NexusClient {
    /// A client for `api_key`. The key is never printed or put in an error.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_key(Some(api_key.into()))
    }

    /// A client for the public endpoints. No `apikey` header is sent, so a public GraphQL query works before the user
    /// saves a key.
    pub fn anonymous() -> Self {
        Self::with_key(None)
    }

    fn with_key(api_key: Option<String>) -> Self {
        let mut client = Client::new("nexus")
            .header("Application-Name", "Gobbo")
            .header("Application-Version", env!("CARGO_PKG_VERSION"));
        if let Some(key) = api_key {
            client = client.header("apikey", key);
        }

        Self { client }
    }

    /// A GET to v2, where the collection archive link lives.
    pub fn get_v2(&self, path: &str) -> Result<String, AppError> {
        self.request(BASE_V2, path, &[], None, Endpoint::Collection)
    }

    /// A GET to v3. `query` is encoded by the request builder, so a key with `+`, `/` or `=` stays intact.
    pub fn get_v3(
        &self,
        path: &str,
        query: &[(&str, &str)],
        endpoint: Endpoint,
    ) -> Result<String, AppError> {
        self.request(BASE_V3, path, query, None, endpoint)
    }

    /// A query to the GraphQL API. The body is the query as JSON.
    pub fn post_graphql(&self, body: &str) -> Result<String, AppError> {
        self.request(GRAPHQL, "", &[], Some(body), Endpoint::GraphQl)
    }

    /// The v1 key check. It is the only v1 request left: v3 has no user endpoint.
    pub fn validate_key(api_key: &str) -> Result<KeyInfo, AppError> {
        #[derive(Deserialize)]
        struct User {
            name: String,
            is_premium: bool,
        }

        let client = Self::new(api_key);
        let text = client.request(BASE_V1, "/users/validate.json", &[], None, Endpoint::Key)?;
        let user: User = serde_json::from_str(&text).map_err(super::error::bad_response)?;
        Ok(KeyInfo {
            name: user.name,
            premium: user.is_premium,
        })
    }

    /// One request. The shared client retries the statuses that may pass; a download link is asked once.
    fn request(
        &self,
        base: &str,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&str>,
        endpoint: Endpoint,
    ) -> Result<String, AppError> {
        let url = format!("{base}{path}");
        // Nexus reports its remaining quota in the response headers. Waiting here only when it is exhausted keeps
        // the normal case at full speed, unlike a fixed gap on every request.
        let budget = http::api_budget("nexus");
        budget.wait();
        let response = body
            .map_or_else(
                || {
                    if endpoint.allows_retry() {
                        self.client.get(&url, query)
                    } else {
                        self.client.get_once(&url, query)
                    }
                },
                |body| self.client.post_json(&url, body),
            )
            .map_err(|error| ApiError::transport(endpoint, error).into_app())?;
        budget.update(response.rate_limit);

        if (200..300).contains(&response.status) {
            return Ok(response.body);
        }

        Err(ApiError::status(endpoint, response.status, Some(&response.body)).into_app())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_link_is_not_retried() {
        assert!(!Endpoint::DownloadLink { has_key: true }.allows_retry());
        assert!(Endpoint::Mod.allows_retry());
    }
}
