//! The Nexus API client. Everything network-facing lives here: the request, the key, the retries, the error mapping
//! and the two API dialects that v3 does not cover (GraphQL facets and the v1 key check).
//!
//! The v3 endpoints are marked Experimental by Nexus. Their wire types stay tolerant (`Option` and `#[serde(default)]`)
//! so an additive change does not break a check or a download.

mod client;
mod error;
mod graphql;
mod models;

pub use client::NexusClient;
pub use error::{Endpoint, bad_response};
pub use graphql::{ModFacet, facets};
pub use models::{Envelope, Named};
