//! Builds the deployment plan: where each file lands, which copy of a contested file is deployed, and the problems that
//! stop a deployment.

mod layout;
mod plan;

#[cfg(test)]
mod problem_tests;
#[cfg(test)]
mod tests;

pub use layout::LayoutRules;
pub use layout::layout_profile;
pub use plan::plan_with_mode;
