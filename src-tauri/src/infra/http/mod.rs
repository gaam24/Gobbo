//! HTTP for every provider: the agent the requests share, the request client with retries and the download of a file
//! with its progress. Nexus and the publishers of game tools both download through here, so the timeouts and the
//! errors are the same.

mod client;
mod download;
mod helpers;
mod pacing;
mod retry;

pub use client::Client;
#[cfg(test)]
pub use download::body_problem;
pub use download::{
    BodyProblem, DownloadInfo, PART_SUFFIX, PartError, download, download_part, network,
    status_error,
};
pub use helpers::{Trace, USER_AGENT, agent, redact, trace, trace_failure, trace_response};
pub use pacing::{RateLimit, api_budget, pacer};
pub use retry::RateRetry;
