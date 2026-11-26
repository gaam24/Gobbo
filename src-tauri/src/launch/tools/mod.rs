//! The tools of a game: running them and the game in the overlay, and installing the prerequisites they need in the
//! Proton prefix. Unpacking a tool's archive lives in `crate::import::tool`.

pub mod prerequisites;
pub mod running;

#[cfg(test)]
mod tests;
