use std::path::Path;

use crate::context::Settings;
use crate::deploy::overlay::Session;
use crate::error::AppError;
use crate::games::GameDef;

/// The variables a game starts with. Under Proton come the chosen Proton options, then the settings' own variables (a
/// variable of both takes the settings' value), then the DLL overrides that make the proxies in `native` load native.
/// `inherited` is the `WINEDLLOVERRIDES` the process would pass on; the override written here keeps its names.
pub fn game_env(
    settings: &Settings,
    through_proton: bool,
    native: &[String],
    inherited: Option<&str>,
) -> Result<Vec<(String, String)>, AppError> {
    let mut env = if through_proton {
        let mut options = crate::domain::proton_options::variables(&settings.proton_options);
        options.extend(crate::domain::proton_options::fps_limit_variables(
            settings.proton_fps_limit,
        ));
        options
    } else {
        Vec::new()
    };

    for (name, value) in crate::domain::arguments::parse_env(&settings.launch_env)? {
        env.retain(|(known, _)| *known != name);
        env.push((name, value));
    }

    if through_proton && !native.is_empty() {
        let existing = env
            .iter()
            .rev()
            .find(|(name, _)| name == "WINEDLLOVERRIDES")
            .map(|(_, value)| value.as_str())
            .or(inherited);
        let dlls: Vec<&str> = native.iter().map(String::as_str).collect();

        if let Some(value) = crate::domain::arguments::native_first_overrides(existing, &dlls) {
            env.retain(|(name, _)| name != "WINEDLLOVERRIDES");
            env.push(("WINEDLLOVERRIDES".to_string(), value));
        }
    }

    Ok(env)
}

/// The DLLs of a launch that load native: the loader DLLs and the proxies of the definition that the launched view of
/// the game has next to the program. `program` is the game's program relative to the game folder. The view is the overlay
/// layer seen from the game folder when the deployment root is the game folder or a folder inside it, so a proxy that a
/// mod places next to the program is found there; a game whose mods live outside the game folder runs its own program
/// folder, which the overlay does not change.
pub fn native_dlls_for_launch(
    game: &GameDef,
    game_root: &Path,
    program: &str,
    session: Option<&Session>,
) -> Vec<String> {
    let dir = crate::games::proxy::program_dir(program);
    session.and_then(Session::game_layers).map_or_else(
        || crate::games::proxy::native_dlls(game, dir, |rel| game_root.join(rel).is_file()),
        |layers| {
            crate::games::proxy::native_dlls(game, dir, |rel| {
                layers.resolve(rel, Path::is_file).is_some()
            })
        },
    )
}
