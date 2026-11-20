//! Report shapes and the CLI convention shared by the overlay probe binary and Gobbo's diagnostics.
//!
//! The probe writes a JSON report and Gobbo reads the same file to show it in the window. Both sides use the types here,
//! so the field names and the report flag cannot drift apart. The shapes are plain data and build on every platform;
//! only the checks themselves need Windows.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The flag that names the report file: `--report <path>`.
pub const REPORT_FLAG: &str = "--report";

#[cfg(windows)]
pub mod probe;

/// The JSON a probe writes when it finishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// Whether every check passed. This is the answer Gobbo shows; it is not recomputed from `checks`.
    pub ok: bool,
    /// The unique probe folder, or `null` when the layers could not be read.
    pub probe: Option<String>,
    /// The layer paths, or `null` when the layers could not be read.
    pub layers: Option<BTreeMap<String, String>>,
    /// The checks, in the order the probe ran them.
    pub checks: Vec<Check>,
}

impl Report {
    /// A report that could not be produced: one failed check carries the reason.
    pub fn failed(name: &str, detail: String) -> Self {
        Self {
            ok: false,
            probe: None,
            layers: None,
            checks: vec![Check {
                name: name.to_string(),
                ok: false,
                detail,
            }],
        }
    }
}

/// One line of a probe report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    /// The name of the check, for example `copy-up`.
    pub name: String,
    /// Whether the check passed.
    pub ok: bool,
    /// Why the check failed; empty when it passed.
    pub detail: String,
}
