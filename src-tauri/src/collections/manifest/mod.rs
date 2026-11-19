//! Reading `collection.json` from a Nexus collection archive and turning it into the plan the installer uses. The
//! manifest is untrusted: every path, count and size is checked before it reaches the profile or the disk.

mod items;
mod model;
mod parse;
mod rules;

pub use parse::{plan_from_api, read_plan};
