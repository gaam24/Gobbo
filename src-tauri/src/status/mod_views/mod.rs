//! The views of each mod and each installed version, as the status shows them.

mod rows;
mod sources;
#[cfg(test)]
mod tests;
mod versions;

pub(in crate::status) use rows::{ExtrasContext, LibraryContext, mod_extras, mod_view};
pub(in crate::status) use sources::source_details;
#[cfg(test)]
pub(in crate::status) use versions::version_view;
pub(in crate::status) use versions::version_views;
