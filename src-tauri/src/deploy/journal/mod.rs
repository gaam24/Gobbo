//! The deployment journal in JSON Lines format, one entry per change on disk. The entry is written before the change,
//! so after an interruption `undo` knows what may have reached the disk.

mod entry;
mod roots;
mod store;

#[cfg(test)]
mod tests;

pub use entry::JournalOp;
pub use roots::JournalRoots;
pub use store::Journal;
