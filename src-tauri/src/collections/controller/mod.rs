//! The backend-owned controller of a collection install. A round imports every member that needs no decision and then
//! pauses with a prompt descriptor; the window only renders that descriptor and sends one decision back. The controller
//! keeps no loop state of its own: the round's progress lives in the persisted session and the cancellation flag, so a
//! restarted window resumes from the same place.

mod classify;
mod model;
mod prompt;
mod run;
#[cfg(test)]
mod tests;

// The façade keeps the controller's public names after the split, including the ones whose only readers live in a
// submodule or in tests.
#[allow(
    unused_imports,
    reason = "the controller root keeps classify as its public surface"
)]
pub use self::classify::classify;
#[allow(
    unused_imports,
    reason = "the controller root keeps the round view types as its public surface"
)]
pub use self::model::{
    COLLECTION_PAGE_EVENT, CollectionDecision, CollectionPageRequest, CollectionPrompt,
    CollectionRoundView, RoundState,
};
pub use self::prompt::resolve_prompt;
pub use self::run::run_round;
