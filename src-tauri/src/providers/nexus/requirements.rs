//! The requirements Nexus lists for a file: the mods it needs and the DLCs, read from the file's version.

use super::api::{Endpoint, NexusClient, bad_response};
use super::files;
use crate::domain::archive::is_number;
use crate::error::AppError;
use serde::Deserialize;

/// A mod that a file of another mod needs, as the file's record names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModRequirement {
    pub mod_id: u32,
    pub name: String,
}

/// What Nexus lists for one downloaded file: its requirements, the DLCs it needs, and the id of the file version. The
/// id lets the next read skip the lookup of the version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRequirements {
    pub version_id: String,
    /// One entry per requirement. An entry lists the mods that satisfy it: one mod, or several that are alternatives,
    /// and any one of them is enough.
    pub requirements: Vec<Vec<ModRequirement>>,
    /// The DLCs the file needs, one entry per requirement, listing the DLCs that satisfy it.
    pub dlcs: Vec<Vec<String>>,
}

/// The requirements of a downloaded file, from Nexus's record of that file (API v3). `known_version` is the file
/// version's global id from an earlier read, when there is one.
pub fn file_requirements(
    client: &NexusClient,
    domain: &str,
    mod_id: u32,
    file_id: u32,
    known_version: Option<&str>,
) -> Result<FileRequirements, AppError> {
    let version_id = match known_version.filter(|id| is_number(id)) {
        Some(version_id) => version_id.to_string(),
        None => files::file_version(client, domain, file_id)?.version_id,
    };
    let text = client.get_v3(
        &format!("/mod-file-versions/{version_id}/dependencies"),
        &[],
        Endpoint::Dependencies,
    )?;
    let (requirements, dlcs) = parse_requirements(&text, mod_id)?;
    Ok(FileRequirements {
        version_id,
        requirements,
        dlcs,
    })
}

/// The requirements of a file version, and the DLC notes: see `parse_requirements`.
type Requirements = (Vec<Vec<ModRequirement>>, Vec<Vec<String>>);

/// Reads the dependency definitions and the DLC definitions of a file version. A definition is one requirement, and
/// all of them must be met. The ranges of one definition are alternatives: any mod among them satisfies it.
fn parse_requirements(text: &str, own_mod: u32) -> Result<Requirements, AppError> {
    #[derive(Deserialize)]
    struct Dependencies {
        dependency_definitions: Vec<Definition>,
        #[serde(default)]
        dlc_dependency_definitions: Vec<DlcDefinition>,
    }
    #[derive(Deserialize)]
    struct DlcDefinition {
        dlc_targets: Vec<DlcTarget>,
    }
    #[derive(Deserialize)]
    struct DlcTarget {
        name: String,
    }
    #[derive(Deserialize)]
    struct Definition {
        ranges: Vec<Range>,
    }
    #[derive(Deserialize)]
    struct Range {
        target_mod_file: TargetFile,
    }
    #[derive(Deserialize)]
    struct TargetFile {
        #[serde(rename = "mod")]
        owner: TargetMod,
    }
    #[derive(Deserialize)]
    struct TargetMod {
        game_scoped_id: String,
        name: String,
    }

    let dependencies: Dependencies = serde_json::from_str(text).map_err(bad_response)?;
    let mut requirements: Vec<Vec<ModRequirement>> = Vec::new();

    for definition in dependencies.dependency_definitions {
        let mut mods: Vec<ModRequirement> = Vec::new();

        for range in definition.ranges {
            let owner = range.target_mod_file.owner;
            let mod_id: u32 = owner.game_scoped_id.parse().unwrap_or(0);

            if !mods.iter().any(|known| known.mod_id == mod_id) {
                mods.push(ModRequirement {
                    mod_id,
                    name: owner.name,
                });
            }
        }
        // Met by the mod itself, so there is nothing to ask of the user.
        if mods.iter().any(|known| known.mod_id == own_mod) {
            continue;
        }

        mods.retain(|known| known.mod_id != 0);

        if mods.is_empty() || requirements.contains(&mods) {
            continue;
        }

        requirements.push(mods);
    }

    let dlcs = dependencies
        .dlc_dependency_definitions
        .into_iter()
        .map(|definition| {
            definition
                .dlc_targets
                .into_iter()
                .map(|target| target.name.trim().to_string())
                .filter(|name| !name.is_empty())
                .collect::<Vec<String>>()
        })
        .filter(|names| !names.is_empty())
        .collect();
    Ok((requirements, dlcs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_lists_the_mods_and_dlcs_it_needs() {
        let text = r#"{"dependency_definitions":[
            {"id":"1","ranges":[{"id":"a","target_mod_file":{"id":"2476822","name":"RED4ext","mod":{"id":"14315125999948","game_scoped_id":"2380","name":"RED4ext"}},"min_version":{"id":"x"},"max_version":null}]},
            {"id":"2","ranges":[
                {"id":"b","target_mod_file":{"id":"8121822","name":"REDGraph 0.1.0","mod":{"id":"1","game_scoped_id":"34967","name":"REDGraph"}},"max_version":null},
                {"id":"c","target_mod_file":{"id":"8121823","name":"REDGraph 0.2.0","mod":{"id":"1","game_scoped_id":"34967","name":"REDGraph"}},"max_version":null}
            ]},
            {"id":"3","ranges":[
                {"id":"d","target_mod_file":{"id":"5","name":"One","mod":{"id":"9","game_scoped_id":"5","name":"One"}}},
                {"id":"e","target_mod_file":{"id":"6","name":"Two","mod":{"id":"10","game_scoped_id":"6","name":"Two"}}}
            ]},
            {"id":"4","ranges":[{"id":"f","target_mod_file":{"id":"7","name":"Itself","mod":{"id":"11","game_scoped_id":"35794","name":"Atmos 2077"}}}]},
            {"id":"5","ranges":[{"id":"g","target_mod_file":{"id":"8","name":"RED4ext","mod":{"id":"12","game_scoped_id":"2380","name":"RED4ext"}}}]},
            {"id":"6","ranges":[]}
        ],"dlc_dependency_definitions":[
            {"id":"d1","dlc_targets":[{"id":"t1","dlc_id":"1","name":"Phantom Liberty"},{"id":"t2","dlc_id":"2","name":"Other"}]}
        ]}"#;
        let (requirements, dlcs) = parse_requirements(text, 35794).unwrap();
        let mod_of = |mod_id: u32, name: &str| ModRequirement {
            mod_id,
            name: name.into(),
        };
        assert_eq!(
            requirements,
            [
                vec![mod_of(2380, "RED4ext")],
                vec![mod_of(34967, "REDGraph")],
                vec![mod_of(5, "One"), mod_of(6, "Two")],
            ]
        );
        assert_eq!(dlcs, [vec!["Phantom Liberty".to_string(), "Other".into()]]);
    }
}
