//! Exporting a profile into a pack.

use super::inspect::pack_error;
use super::model::ARCHIVES;
use super::model::FORMAT;
use super::model::MANIFEST;
use super::model::Manifest;
use super::model::PackArchives;
use super::model::PackExportView;
use super::model::PackGroup;
use super::model::PackOptions;
use super::model::PackProfile;
use super::model::PackSeparator;
use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::domain::pack::{PackArchive, PackMod};
use crate::domain::profiles::Profile;

use crate::domain::library::Library;
use crate::error::AppError;
use crate::error::io_at;
use crate::launch::steps::load_mod_steps;
use crate::library::FamilyFields;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;

/// Writes the chosen profiles as a pack to `path`.
pub fn export_pack(
    root: &Data,
    game_id: &str,
    path: &Path,
    options: &PackOptions,
) -> Result<PackExportView, AppError> {
    let Loaded {
        data,
        library,
        profiles,
        ..
    } = load_game(root, game_id)?;
    let steps = load_mod_steps(&data)?;

    if options.info.name.trim().is_empty() || options.info.version.trim().is_empty() {
        return Err(AppError::user("pack.info_missing"));
    }

    if options.profiles.is_empty() {
        return Err(AppError::user("pack.no_profiles"));
    }

    let mut info = options.info.clone();

    if info.id.trim().is_empty() {
        info.id = crate::domain::naming::slug(&info.name);
    }

    let mut manifest = Manifest {
        format: FORMAT,
        game: game_id.to_string(),
        info,
        profiles: Vec::new(),
    };
    let mut bundled: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut mods = 0;

    for id in &options.profiles {
        let profile = profiles.get(id)?;
        let mut pack = pack_profile(&data, &library, &steps, profile);

        for entry in &mut pack.mods {
            let has_source = entry.has_source();
            let Some(archive) = &mut entry.archive else {
                continue;
            };
            // Only the archives this export writes count as inside the pack.
            archive.bundled = false;
            let wanted = match options.archives {
                PackArchives::None => false,
                PackArchives::Unlisted => !has_source,
                PackArchives::All => true,
            };
            let file = library.archive_file_of(&archive.sha256);

            if let (true, Some(file)) = (wanted, file)
                && data.archives().join(&file).is_file()
            {
                archive.bundled = true;
                bundled.insert(archive.sha256.clone(), (archive.name.clone(), file));
            }
        }

        mods += pack.mods.len();
        manifest.profiles.push(pack);
    }

    let output = fs::File::create(path).map_err(io_at(path))?;
    let mut writer = zip::ZipWriter::new(output);
    // The manifest is machine-read, so compact JSON is enough and avoids the pretty printer and its larger buffer.
    let text = serde_json::to_vec(&manifest).map_err(AppError::unexpected)?;
    writer
        .start_file(MANIFEST, zip::write::SimpleFileOptions::default())
        .map_err(pack_error)?;
    writer.write_all(&text).map_err(io_at(path))?;
    // Archives are already compressed, so they are stored as they are.
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (sha256, (name, file)) in &bundled {
        writer
            .start_file(format!("{ARCHIVES}{sha256}/{name}"), stored)
            .map_err(pack_error)?;
        let source = data.archives().join(file);
        let mut input = fs::File::open(&source).map_err(io_at(&source))?;
        io::copy(&mut input, &mut writer).map_err(io_at(path))?;
    }

    writer.finish().map_err(pack_error)?;
    Ok(PackExportView {
        mods: u32::try_from(mods).unwrap_or(u32::MAX),
        archives: u32::try_from(bundled.len()).unwrap_or(u32::MAX),
    })
}

/// One profile in pack form.
pub(super) fn pack_profile(
    data: &Data,
    library: &Library,
    steps: &crate::domain::mod_steps::ModSteps,
    profile: &Profile,
) -> PackProfile {
    let listed: Vec<(String, PackMod)> = profile
        .mods
        .iter()
        .map(|entry| {
            let record = library.versions.get(&entry.folder);
            let family = library.families.get(&entry.id);
            let fields = FamilyFields::of(family);
            let archive = record.and_then(|record| {
                let sha256 = record.archive.clone()?;
                let stored = library.archives.get(&sha256)?;
                let size =
                    fs::metadata(data.archives().join(&stored.file)).map_or(0, |meta| meta.len());
                Some((
                    PackArchive {
                        name: stored.original_name.clone(),
                        sha256,
                        size,
                        bundled: false,
                    },
                    stored.source_files.clone(),
                    stored.source_parts.clone(),
                ))
            });
            let mut sources = family
                .map(|family| family.sources.clone())
                .unwrap_or_default();
            // The picture file is local to the author, and the check time is the author's. The receiver checks the
            // page again.
            for source in sources.values_mut() {
                if let Some(meta) = &mut source.meta {
                    meta.picture = None;
                    meta.checked_at = None;
                }
            }

            let requirements = family
                .map(|family| family.requirements.clone())
                .unwrap_or_default();
            let source_dlcs = family
                .map(|family| family.source_dlcs.clone())
                .unwrap_or_default();
            let pack = PackMod {
                name: family.map_or_else(|| entry.id.clone(), |family| family.name.clone()),
                version: record.and_then(|record| record.info.version.clone()),
                family: entry.id.clone(),
                enabled: entry.enabled,
                priority: entry.priority,
                sources,
                source_files: archive
                    .as_ref()
                    .map(|(_, files, _)| files.clone())
                    .unwrap_or_default(),
                source_parts: archive.as_ref().map_or_else(
                    || {
                        family
                            .map(|family| {
                                family
                                    .sources
                                    .iter()
                                    .filter_map(|(provider, source)| {
                                        Some((provider.clone(), source.part_id.clone()?))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    },
                    |(_, _, parts)| parts.clone(),
                ),
                part_name: family
                    .and_then(|family| {
                        family
                            .sources
                            .values()
                            .find_map(|source| source.part_name.clone())
                    })
                    .or_else(|| family.map(|family| family.name.clone())),
                archive: archive.map(|(archive, _, _)| archive),
                fomod: record
                    .map(|record| record.fomod.clone())
                    .unwrap_or_default(),
                fomod_installer: record.is_some_and(|record| record.installer_choices().is_some()),
                tags: family
                    .map(|family| family.tags.iter().cloned().collect())
                    .unwrap_or_default(),
                instructions: fields.instructions,
                requires: fields.requires,
                requirements,
                source_dlcs,
                notes: fields.notes,
                incompatible: fields.incompatible,
                overrides: fields.overrides,
                steps: steps.mods.get(&entry.folder).cloned(),
            };
            (entry.id.clone(), pack)
        })
        .collect();
    // The waiting mods go where they sit once the library has them: above the mod they name, in the order of the pack.
    // One pass builds the anchors, and each waiting mod goes into a small bucket before its anchor. Inserting into a
    // Vec for every waiting mod instead would scan and shift the whole list each time (quadratic when most wait).
    let count = listed.len();
    let anchors: BTreeMap<&str, usize> = listed
        .iter()
        .enumerate()
        .map(|(at, (key, _))| (key.as_str(), at))
        .collect();
    let mut waiting_before: Vec<Vec<(String, PackMod)>> = vec![Vec::new(); count + 1];

    for waiting in &profile.waiting {
        let at = waiting
            .before
            .as_deref()
            .and_then(|id| anchors.get(id).copied())
            .unwrap_or(count);
        waiting_before[at].push((waiting.entry.family.clone(), waiting.entry.clone()));
    }

    let mut ordered = Vec::with_capacity(count + profile.waiting.len());

    for (at, entry) in listed.into_iter().enumerate() {
        for waiting in std::mem::take(&mut waiting_before[at]) {
            ordered.push(waiting);
        }

        ordered.push(entry);
    }

    ordered.append(&mut waiting_before[count]);
    let listed = ordered;
    let index_of: BTreeMap<&str, usize> = listed
        .iter()
        .enumerate()
        .map(|(at, (key, _))| (key.as_str(), at))
        .collect();
    let separators = profile
        .separators
        .iter()
        .map(|separator| PackSeparator {
            name: separator.name.clone(),
            before: separator
                .before
                .as_deref()
                .and_then(|id| index_of.get(id).copied()),
        })
        .collect();
    let winners = profile
        .winners
        .iter()
        .filter_map(|(path, family)| Some((path.clone(), *index_of.get(family.as_str())?)))
        .collect();
    let groups = profile
        .groups
        .iter()
        .map(|group| PackGroup {
            name: group.name.clone(),
            kind: group.kind,
            language: group.language.clone(),
            // A member family that is not in the pack has no index and is left out.
            members: group
                .members
                .iter()
                .filter_map(|family| index_of.get(family.as_str()).copied())
                .collect(),
        })
        .collect();
    PackProfile {
        name: profile.name.clone(),
        mods: listed.into_iter().map(|(_, pack)| pack).collect(),
        separators,
        winners,
        groups,
        user_data: profile.user_data.clone(),
    }
}
