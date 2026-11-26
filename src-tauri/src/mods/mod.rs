//! The mod-list commands and the per-mod steps the inspector edits: status, details, imports, library edits and
//! per-mod settings. The engines behind the commands live in `status`, `import` and `library`; this module owns the
//! IPC and the `mod_steps.toml` edit, whose load and save engine lives in `launch::steps`.

pub mod commands;
pub mod steps;
