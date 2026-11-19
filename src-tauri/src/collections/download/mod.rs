//! Installing the selected members. The automatic path downloads through the provider; the guided path asks the queue
//! for one owned download and imports it with the collection's FOMOD choices. Bundles, direct URLs and patches are
//! handled here for both paths.

mod import;
mod loaded;
mod prefetch;
mod rounds;
mod source;

pub use rounds::{import_file, install_automatic, prepare_item};
pub use source::provider_source;
