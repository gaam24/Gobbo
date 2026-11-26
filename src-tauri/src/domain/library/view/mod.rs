//! A read-only view of the library, built once for a request: the versions of each family in the order the UI shows
//! them, the default version and the source indexes. The status, the update check and the mod views all need these
//! indexes; building them once avoids sorting the versions again for every family and every lookup.

mod find;
mod index;
#[cfg(test)]
mod tests;

pub use find::{FindQuery, FoundBy, MatchPolicy};
pub use index::LibraryView;

/// The families of one page as an iterator: one inline value or the borrowed `Vec` of a group. Keeping the result in
/// this type means the lookup key does not leak into the returned lifetime.
pub struct PageFamiliesIter<'a, 'b> {
    one: Option<&'a str>,
    many: std::slice::Iter<'b, &'a str>,
}

impl<'a> Iterator for PageFamiliesIter<'a, '_> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.one.take().or_else(|| self.many.next().copied())
    }
}
