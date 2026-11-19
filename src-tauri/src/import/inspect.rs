//! Reading a FOMOD installer out of an archive for the wizard.

use super::model::{FomodCondition, FomodDependencies, FomodDependency, ZipInspect};
use crate::context::Data;
use crate::error::{AppError, ErrorKind};
use crate::games;
use crate::import::fomod;
use crate::import::fomod::{Condition, Dependencies, Dependency, normalize_path_into};
use crate::infra::archive;

use super::wizard::{fomod_config_entry, read_config};
use super::{FomodGroupView, FomodOptionView, FomodStepView};

/// Maps a parsed `<visible>` tree to the view the wizard evaluates. File paths are normalized here so the window never
/// has to repeat the FOMOD path rules.
fn dependencies_view(dependencies: &Dependencies) -> FomodDependencies {
    let convert = |condition: &Condition| match condition {
        Condition::Dependency(Dependency::File { file, active }) => {
            let mut normalized = String::new();
            normalize_path_into(file, &mut normalized);
            FomodCondition::Dependency(FomodDependency::File {
                file: normalized,
                active: *active,
            })
        }
        Condition::Dependency(Dependency::Flag { flag, value }) => {
            FomodCondition::Dependency(FomodDependency::Flag {
                flag: flag.clone(),
                value: value.clone(),
            })
        }
        Condition::Dependency(Dependency::Game) => {
            FomodCondition::Dependency(FomodDependency::Game)
        }
        Condition::Group(group) => FomodCondition::Group(dependencies_view(group)),
    };
    let children = match dependencies {
        Dependencies::All(children) | Dependencies::Any(children) => {
            children.iter().map(convert).collect()
        }
    };
    match dependencies {
        Dependencies::All(_) => FomodDependencies::All(children),
        Dependencies::Any(_) => FomodDependencies::Any(children),
    }
}

/// Loads the archive and recognizes a FOMOD installer. Reads only the list of entries and one XML file.
pub fn inspect(_root: &Data, game_id: &str, zip: &std::path::Path) -> Result<ZipInspect, AppError> {
    let _ = games::load(game_id)?;
    let reader = archive::Archive::open(zip)?;
    let Some(config_entry) = fomod_config_entry(&reader) else {
        return Ok(ZipInspect::Plain);
    };
    let text = read_config(&reader, config_entry)?;
    let config = fomod::parse(&text)?;
    let steps = config
        .steps
        .iter()
        .map(|step| FomodStepView {
            name: step.name.clone(),
            visible: step.visible.as_ref().map(dependencies_view),
            groups: step
                .groups
                .iter()
                .map(|group| FomodGroupView {
                    name: group.name.clone(),
                    kind: group.kind,
                    options: group
                        .options
                        .iter()
                        .map(|option| FomodOptionView {
                            name: option.name.clone(),
                            description: option.description.clone(),
                            kind: option.kind,
                            flags: option.flags.clone(),
                            destinations: option
                                .files
                                .iter()
                                .map(|pattern| {
                                    let mut normalized = String::new();
                                    normalize_path_into(&pattern.destination, &mut normalized);
                                    normalized
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    Ok(ZipInspect::Fomod {
        module_name: config.module_name,
        steps,
    })
}

/// The wizard's answer for the installer of an archive under a selection (`None` is the default selection). It reads
/// the installer the way `inspect` does; a plain archive has no installer to answer for.
pub fn fomod_state(
    _root: &Data,
    game_id: &str,
    zip: &std::path::Path,
    selection: Option<&[fomod::Choice]>,
) -> Result<fomod::FomodState, AppError> {
    let _ = games::load(game_id)?;
    let reader = archive::Archive::open(zip)?;
    let Some(config_entry) = fomod_config_entry(&reader) else {
        return Err(AppError::new("import.not_fomod").kind(ErrorKind::Permanent));
    };
    let text = read_config(&reader, config_entry)?;
    let config = fomod::parse(&text)?;
    Ok(fomod::installer_state(&config, selection))
}
