//! The requirement closure: everything one mod needs, directly and through the mods it needs, with the alternatives
//! and the cycles visible before anything is downloaded. The same plan drives the bulk install, so the preview and the
//! action cannot disagree.

mod install;
mod model;
mod plan;

pub use install::{install_cancellable, install_one_cancellable};
pub use model::{RequirementInstallView, RequirementPlan};
// The tests read the plan types through this root; production code imports them from `model`.
#[allow(
    unused_imports,
    reason = "the integration tests read these types through the module root"
)]
pub use model::{RequirementAction, RequirementStep};
pub use plan::plan;
