//! The errors shared by the provider adapters and the routing helpers.

use crate::error::{AppError, ErrorKind, NO_GAME_KEY};

/// The error for an action a provider does not offer.
pub(super) fn unsupported() -> AppError {
    AppError::user("source.unsupported")
}

/// The code of a direct download that needs a paid account. The interface points the user to the mod's page instead.
/// Defined once in `error.rs` so every entitlement failure uses the same code.
pub use crate::error::PAYMENT_REQUIRED;

/// The provider has no key for the game: the game definition names none.
pub fn no_game_key(provider: &str) -> AppError {
    AppError::user(NO_GAME_KEY).with("provider", provider)
}

/// A link that does not belong to a mod provider Gobbo knows.
pub fn bad_link() -> AppError {
    AppError::user("source.bad_link")
}

/// A response that is not what the API promises. A retry gives the same answer, so it is not retryable.
pub fn bad_response(code: &str, detail: impl ToString) -> AppError {
    AppError::new(code)
        .with("detail", detail)
        .kind(ErrorKind::Permanent)
}

/// The error for an action the provider `id` does not offer.
pub fn unsupported_by(id: &str) -> AppError {
    unsupported().with("provider", id)
}
