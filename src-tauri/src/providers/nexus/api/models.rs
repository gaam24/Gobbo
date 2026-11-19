//! Small shared pieces of the API wire format: dates, the problem body, and the wrappers around a v3 payload.

use serde::Deserialize;

pub use crate::infra::clock::unix_time;

/// A v3 answer, which carries its payload under `data`. A missing `data` reads as `None` when `T` is an option.
#[derive(Debug, Clone, Deserialize)]
pub struct Envelope<T> {
    pub data: T,
}

/// A v3 object that names itself, such as an uploader, a category, a tag or a file group.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Named {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// The body of a failed v3 request (`application/problem+json`).
#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct ProblemDetails {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}
