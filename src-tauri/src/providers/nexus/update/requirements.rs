//! The requirements Nexus lists for a family: the newest file's, the mod's own, and the fallback to an older file.

use super::Listed;
use super::lookup::Lookup;
use crate::infra::clock::now_seconds;
use crate::providers::nexus::ModRequirement;
use crate::providers::nexus::api::{ModFacet, NexusClient};
use crate::providers::nexus::files::{ModFile, mod_files};
use crate::providers::nexus::{NexusRequirement, NexusRequiresSync};

pub(super) use crate::providers::nexus::files::newest_match;

/// A requirement read from a file is kept for a week before the file is read again.
const REQUIRES_MAX_AGE: u32 = 7 * 24 * 60 * 60;

/// The requirements Nexus lists for the family: those of its newest file, and those the mod lists for itself. When the
/// newest file lists none, those of an older file are taken. `None` when they were read within the last week, or when
/// a request failed: the saved ones stay then.
pub(super) fn listed_requirements(
    client: &NexusClient,
    domain: &str,
    item: &Lookup,
    mod_uid: Option<&str>,
    file_id: Option<u32>,
    facet: Option<&ModFacet>,
) -> Option<Listed> {
    let now = now_seconds();
    let same_file =
        file_id.and_then(|file_id| item.known.as_ref().filter(|known| known.file_id == file_id));
    if let Some(known) = same_file
        && known.mod_level
        && now.saturating_sub(known.checked_at) < REQUIRES_MAX_AGE
    {
        return None;
    }
    let mod_level: Vec<ModRequirement> = facet
        .map(|facet| {
            facet
                .requirements
                .iter()
                .map(|requirement| ModRequirement {
                    mod_id: requirement.mod_id,
                    name: requirement.name.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let Some(file_id) = file_id else {
        // No file of the mod is known to Nexus, so only the list the mod keeps for itself can be read.
        facet?;
        return Some(Listed {
            requires: with_mod_level(Vec::new(), &mod_level, item.nexus_id),
            dlcs: Vec::new(),
            sync: None,
        });
    };
    let read = crate::providers::nexus::file_requirements(
        client,
        domain,
        item.nexus_id,
        file_id,
        same_file.map(|known| known.version_id.as_str()),
    )
    .ok()?;
    let older = if read.requirements.is_empty() && mod_level.is_empty() {
        older_listing(client, domain, item, mod_uid, file_id)
    } else {
        None
    };
    let older_version = older.as_ref().map(|older| older.version.clone());
    let (requires, dlcs) = match older {
        Some(older) => (as_requirements(older.requirements), older.dlcs),
        None => (as_requirements(read.requirements), read.dlcs),
    };
    Some(Listed {
        requires: with_mod_level(requires, &mod_level, item.nexus_id),
        dlcs,
        sync: Some(NexusRequiresSync {
            file_id,
            version_id: read.version_id,
            checked_at: now,
            mod_level: facet.is_some(),
            older_version,
        }),
    })
}

/// How many older files a read looks at, newest first, before it gives up.
const OLDER_FILES_MAX: usize = 3;

/// The requirements of an older file of the mod, and the version it is.
struct Older {
    requirements: Vec<Vec<ModRequirement>>,
    dlcs: Vec<Vec<String>>,
    version: String,
}

/// The requirements of the newest of the few older files that lists some. A file whose read fails is passed over.
/// `None` when the files cannot be listed or none of those looked at lists any.
fn older_listing(
    client: &NexusClient,
    domain: &str,
    item: &Lookup,
    mod_uid: Option<&str>,
    newest: u32,
) -> Option<Older> {
    let mut files: Vec<ModFile> = mod_files(client, mod_uid?)
        .ok()?
        .into_iter()
        .filter(|file| file.file_id < newest)
        .collect();
    files.sort_by_key(|file| std::cmp::Reverse(file.file_id));
    files.into_iter().take(OLDER_FILES_MAX).find_map(|file| {
        let read = crate::providers::nexus::file_requirements(
            client,
            domain,
            item.nexus_id,
            file.file_id,
            None,
        )
        .ok()?;
        (!read.requirements.is_empty()).then(|| Older {
            requirements: read.requirements,
            dlcs: read.dlcs,
            version: file.version.unwrap_or(file.file_name),
        })
    })
}

/// The requirements the mod lists for itself, added to those of its file where the file does not list them already.
/// The mod itself is never one of them.
pub(super) fn with_mod_level(
    mut requires: Vec<NexusRequirement>,
    mod_level: &[ModRequirement],
    own: u32,
) -> Vec<NexusRequirement> {
    for requirement in mod_level {
        if requirement.mod_id != own
            && !requires
                .iter()
                .any(|known| known.mod_id == requirement.mod_id)
        {
            requires.push(NexusRequirement {
                mod_id: requirement.mod_id,
                name: requirement.name.clone(),
                from_nexus: true,
                group: None,
                ignored: false,
            });
        }
    }
    requires
}

/// The file an archive came from, for an archive that came in without a Nexus link: the newest of the mod's files
/// whose name is the archive's name. `None` when no name matches, or the mod's files cannot be listed.
pub(super) fn match_file(
    client: &NexusClient,
    item: &Lookup,
    mod_uid: Option<&str>,
) -> Option<(String, u32, Option<String>)> {
    if item.unmatched.is_empty() {
        return None;
    }
    let files = mod_files(client, mod_uid?).ok()?;
    let (digest, file) = newest_match(&files, &item.unmatched)?;
    Some((digest, file.file_id, Some(file.mod_file_id)))
}

/// The requirements of the family, from what Nexus lists. Alternatives share a group, so any one of them satisfies
/// the requirement.
pub(super) fn as_requirements(requirements: Vec<Vec<ModRequirement>>) -> Vec<NexusRequirement> {
    requirements
        .into_iter()
        .enumerate()
        .flat_map(|(index, mods)| {
            let group = (mods.len() > 1).then(|| u32::try_from(index).unwrap_or(u32::MAX));
            mods.into_iter().map(move |requirement| NexusRequirement {
                mod_id: requirement.mod_id,
                name: requirement.name,
                from_nexus: true,
                group,
                ignored: false,
            })
        })
        .collect()
}
