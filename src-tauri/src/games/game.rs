//! The methods on the definition types that belong to this module rather than to `definition/` itself.

use std::path::{Path, PathBuf};

use super::definition::{GameDef, LoadOrder, Tool, UserData};
use super::paths;

impl GameDef {
    /// The user-data sets declared by this game, in definition order.
    pub fn user_data_sets(&self) -> &[UserData] {
        &self.user_data
    }

    /// Parses a definition without resolving its loader or checking it. `load` and `load_dir` do both.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// The folder of the game's order file on this computer, when the game has one and the folder already exists.
    /// `prefix` and `under_proton` describe the install being added.
    pub fn find_order_dir(
        &self,
        game_root: &Path,
        home: &Path,
        prefix: Option<PathBuf>,
        under_proton: bool,
    ) -> Option<PathBuf> {
        let order = self.settings_order()?;
        let context = paths::Context {
            game_root: game_root.to_path_buf(),
            prefix,
            under_proton,
            home: home.to_path_buf(),
            settings: None,
        };
        let file = context.resolve(&order.file).ok()?;
        let dir = file.parent()?;
        dir.is_dir().then(|| dir.to_path_buf())
    }
}

impl GameDef {
    /// The tool of the game that is this mod on the provider, when the definition names one.
    pub fn tool_for_source(&self, provider: &str, mod_id: &str) -> Option<&Tool> {
        self.tools.iter().find(|tool| {
            tool.source.as_ref().is_some_and(|source| {
                source.provider.as_str() == provider && source.mod_id == mod_id
            })
        })
    }
}

impl LoadOrder {
    /// The order file in the folder the user chose. It keeps the file name from the definition.
    pub fn file_in(&self, dir: &Path) -> PathBuf {
        let name = self.file.rsplit(['/', '\\']).next().unwrap_or(&self.file);
        dir.join(name)
    }
}
