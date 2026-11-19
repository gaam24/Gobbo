//! HTTP for every provider: the agent the requests share, the request client with retries and the download of a file
//! with its progress. Nexus and the publishers of game tools both download through here, so the timeouts and the
//! errors are the same.

mod client;
mod download;
mod helpers;
mod pacing;
mod retry;

pub use client::Client;
pub use download::{
    BodyProblem, DownloadInfo, PART_SUFFIX, body_problem, download, download_with_info, network,
    status_error, temporary_download,
};
pub use helpers::{Trace, USER_AGENT, agent, redact, trace, trace_failure, trace_response};
pub use pacing::{RateLimit, api_budget, pacer};
pub use retry::RateRetry;
