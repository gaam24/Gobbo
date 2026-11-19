use crate::domain::collection::{CollectionItem, CollectionSource};
use crate::domain::source::SourceRef;

/// The provider file a member names, with its part when the plan knows it, as every download and import of the member
/// uses it.
pub fn provider_source(item: &CollectionItem) -> Option<SourceRef> {
    let CollectionSource::Provider {
        provider,
        mod_id,
        file_id,
        part_id,
    } = &item.source
    else {
        return None;
    };
    let source = SourceRef::new(provider.as_str(), mod_id.clone()).with_file(file_id.clone());
    Some(match part_id {
        Some(part) => source.with_part(part.clone()),
        None => source,
    })
}
