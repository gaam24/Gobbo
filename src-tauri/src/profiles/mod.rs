//! Profiles: the use cases that change a profile's list (save, create, switch, delete), the bisect steps, the comparison,
//! the order rules, the MO2 import and the history preview. The per-profile data they change (the copy list, the
//! user-data sets, the switching of both and the kept history) is in `context::profile`.

pub mod actions;
pub mod bisect;
pub mod commands;
pub mod community;
pub mod compare;
pub mod edits;
pub mod history;
pub mod mo2;
pub mod order;
mod transfer;
