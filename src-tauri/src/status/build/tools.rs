use std::path::Path;

use crate::context::is_dir;
use crate::games;
use crate::games::GameDef;
use crate::launch::tools::prerequisites::missing_prerequisites;
use crate::status::views::ToolView;

/// The tools of the definition. A tool is found when one of its `detect` files is in the game folder.
pub(super) fn tool_views(game: &GameDef, game_root: &str) -> Vec<ToolView> {
    game.tools
        .iter()
        .map(|tool| ToolView {
            id: tool.id.clone(),
            name: tool.name.clone(),
            output: tool.output.clone(),
            folder: tool
                .detect
                .iter()
                .find_map(|relative| tool_folder(game_root, relative)),
            source: tool.source.clone(),
            missing: missing_prerequisites(game, tool, game_root),
        })
        .collect()
}

/// The folder with the program at `relative` inside the game folder. The path must stay in the game folder.
pub fn tool_folder(game_root: &str, relative: &str) -> Option<String> {
    if !is_dir(game_root) || !games::is_game_relative(relative) {
        return None;
    }
    let file = Path::new(game_root.trim()).join(relative);
    file.is_file()
        .then(|| file.parent().map(|folder| folder.display().to_string()))
        .flatten()
}
