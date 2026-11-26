//! Turning an HTTP answer into the one error the window shows. The same status means different things on a mod page
//! and on a download link, so the endpoint is part of the mapping.

use super::models::ProblemDetails;
use crate::error::{AppError, ErrorKind};

/// Which endpoint failed. It decides what a 403 or a 404 means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    /// The v1 key check.
    Key,
    /// A mod page or a v3 mod endpoint.
    Mod,
    /// A file, a file version or a file list.
    File,
    /// A signed download link. `has_key` says whether the NXM link carried a free-user key.
    DownloadLink { has_key: bool },
    /// The dependencies of one file version.
    Dependencies,
    /// A collection revision or its archive link.
    Collection,
    /// The v2 GraphQL endpoint.
    GraphQl,
}

impl Endpoint {
    /// Whether a failed request may be repeated. Asking for a download link records a download on Nexus, so that
    /// request is never repeated automatically.
    pub(super) const fn allows_retry(self) -> bool {
        !matches!(self, Self::DownloadLink { .. })
    }

    /// The `kind` param of the one Nexus 404 code. It keeps the endpoint's meaning without a code per endpoint.
    const fn not_found_kind(self) -> &'static str {
        match self {
            Self::Mod | Self::GraphQl => "mod",
            Self::Collection => "collection",
            Self::File | Self::Dependencies | Self::DownloadLink { .. } => "file",
            Self::Key => "key",
        }
    }
}

/// A failed request before it is turned into an `AppError`.
#[derive(Debug)]
pub(super) struct ApiError {
    /// The HTTP status, or `None` when the connection itself failed.
    pub(super) status: Option<u16>,
    pub(super) problem: Option<ProblemDetails>,
    pub(super) endpoint: Endpoint,
    pub(super) detail: Option<String>,
}

impl ApiError {
    /// A connection that never produced an HTTP answer.
    pub(super) fn transport(endpoint: Endpoint, detail: impl ToString) -> Self {
        Self {
            status: None,
            problem: None,
            endpoint,
            detail: Some(detail.to_string()),
        }
    }

    /// An HTTP answer with a status outside 2xx.
    pub(super) fn status(endpoint: Endpoint, status: u16, body: Option<&str>) -> Self {
        let problem = body.and_then(|text| serde_json::from_str(text).ok());
        let detail = body
            .map(str::trim)
            .filter(|text| !text.is_empty() && !text.starts_with('<') && text.len() <= 300)
            .map(str::to_string);
        Self {
            status: Some(status),
            problem,
            endpoint,
            detail,
        }
    }

    /// The error the window shows and the frontend translates.
    pub(super) fn into_app(self) -> AppError {
        let (code, kind) = self.code();
        let mut error = AppError::new(code).kind(kind);

        if code == crate::providers::nexus::NEXUS_NOT_FOUND {
            error = error.with("kind", self.endpoint.not_found_kind());
        }

        if code == crate::providers::nexus::KEY_REJECTED {
            error = error.with("provider", super::super::ID);
        }

        if let Some(status) = self.status {
            error = error.with("code", status);
        }

        let detail = self
            .problem
            .as_ref()
            .and_then(|problem| problem.detail.clone().or_else(|| problem.title.clone()))
            .or(self.detail);
        if let Some(detail) = detail.filter(|text| !text.is_empty()) {
            error = error.with("detail", crate::infra::http::redact(&detail));
        }

        error
    }

    /// The stable code and what the user can do about it.
    const fn code(&self) -> (&'static str, ErrorKind) {
        match (self.status, self.endpoint) {
            (None, _) => ("nexus.network", ErrorKind::Retryable),
            (Some(400), Endpoint::DownloadLink { .. }) => {
                ("nexus.link_invalid", ErrorKind::UserAction)
            }
            (Some(400 | 422), _) => ("nexus.bad_request", ErrorKind::Permanent),
            (Some(401), _) | (Some(403 | 404), Endpoint::Key) => {
                (crate::providers::nexus::KEY_REJECTED, ErrorKind::UserAction)
            }
            // A signed link asked without a free-user key means the account tier is the problem, not the file.
            (Some(403), Endpoint::DownloadLink { has_key: false }) => {
                (crate::error::PAYMENT_REQUIRED, ErrorKind::UserAction)
            }
            // With a free-user key the tier was already accepted; a refusal here is moderation or quarantine.
            (Some(403), Endpoint::DownloadLink { has_key: true }) => (
                crate::providers::nexus::DOWNLOAD_UNAVAILABLE,
                ErrorKind::UserAction,
            ),
            (Some(403), Endpoint::Collection) => {
                ("nexus.collection_forbidden", ErrorKind::UserAction)
            }
            (Some(403), _) => ("nexus.forbidden", ErrorKind::UserAction),
            // All 404s are the same meaning ("not found"); the `kind` param names the endpoint in `into_app`.
            (Some(404), _) => (
                crate::providers::nexus::NEXUS_NOT_FOUND,
                ErrorKind::Permanent,
            ),
            (Some(410), Endpoint::DownloadLink { .. }) => {
                ("nexus.link_expired", ErrorKind::UserAction)
            }
            (Some(429), _) => ("nexus.rate_limited", ErrorKind::Retryable),
            (Some(451), _) => ("nexus.adult_content", ErrorKind::UserAction),
            (Some(500..=599), _) => ("nexus.unavailable", ErrorKind::Retryable),
            (Some(_), _) => ("nexus.status", ErrorKind::Permanent),
        }
    }
}

/// A response that is not what the API promises. A retry gives the same answer, so it is not retryable.
pub fn bad_response(detail: impl ToString) -> AppError {
    crate::providers::bad_response("nexus.bad_response", detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_link_explains_an_expired_or_missing_key() {
        let expired =
            ApiError::status(Endpoint::DownloadLink { has_key: true }, 410, None).into_app();
        assert_eq!(expired.code, "nexus.link_expired");
        assert_eq!(expired.kind, ErrorKind::UserAction);
        let invalid =
            ApiError::status(Endpoint::DownloadLink { has_key: true }, 400, None).into_app();
        assert_eq!(invalid.code, "nexus.link_invalid");
        let premium =
            ApiError::status(Endpoint::DownloadLink { has_key: false }, 403, None).into_app();
        assert_eq!(premium.code, crate::error::PAYMENT_REQUIRED);
        assert_eq!(premium.kind, ErrorKind::UserAction);
    }

    #[test]
    fn a_quarantined_file_is_not_a_rejected_api_key() {
        let error =
            ApiError::status(Endpoint::DownloadLink { has_key: true }, 403, None).into_app();
        assert_eq!(error.code, crate::providers::nexus::DOWNLOAD_UNAVAILABLE);
    }

    #[test]
    fn every_404_uses_one_code_with_the_endpoint_as_its_kind() {
        let missing_mod = ApiError::status(Endpoint::Mod, 404, None).into_app();
        assert_eq!(missing_mod.code, crate::providers::nexus::NEXUS_NOT_FOUND);
        assert_eq!(missing_mod.params["kind"], "mod");
        let missing_file = ApiError::status(Endpoint::File, 404, None).into_app();
        assert_eq!(missing_file.code, crate::providers::nexus::NEXUS_NOT_FOUND);
        assert_eq!(missing_file.params["kind"], "file");
        let missing_collection = ApiError::status(Endpoint::Collection, 404, None).into_app();
        assert_eq!(
            missing_collection.code,
            crate::providers::nexus::NEXUS_NOT_FOUND
        );
        assert_eq!(missing_collection.params["kind"], "collection");
    }

    #[test]
    fn a_problem_body_gives_the_detail() {
        let error = ApiError::status(
            Endpoint::Mod,
            403,
            Some(r#"{"title":"Forbidden","detail":"adult content is hidden"}"#),
        )
        .into_app();
        assert_eq!(error.code, "nexus.forbidden");
        assert_eq!(error.params["detail"], "adult content is hidden");
    }

    #[test]
    fn a_signed_link_never_reaches_the_error() {
        let error = ApiError::transport(
            Endpoint::DownloadLink { has_key: true },
            "https://api.nexusmods.com/v3/x?key=secret&expires=123",
        )
        .into_app();
        let text = error.params["detail"].as_str();
        assert!(!text.contains("secret"), "{text}");
        assert!(!text.contains("123"), "{text}");
    }
}
