//! The layout rules of an import: which entry of the archive is a mod file, which folder it belongs to, and whether it is
//! allowed in the library at all.

mod classify;
mod plan;
mod units;

pub(super) use plan::{LayoutPlan, attach_mirrored};
pub(super) use units::UnitIndex;
