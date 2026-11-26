//! The shape of `game.toml`. Every table rejects keys it does not know, so a typo fails when the definition loads
//! instead of being ignored. Rules that need several fields together are in `validate/`.
//!
//! One model covers every game. Mods reach the game through two kinds of place, and a game may use both:
//! - a slot (`[[deploy.slots]]`): a folder where each mod gets a folder of its own, or puts its files directly when
//!   the slot is flat (`Mods/<mod>/...`, `BepInEx/plugins/<mod>/...`, `Content/Paks/~mods/*.pak`);
//! - a mirror (`[[deploy.mirror]]`): a game folder whose files keep their game paths (`archive/pc/mod/...`, `DLC/...`,
//!   `Data/...`). Two mods that write the same path conflict, and the list or the user picks the winner.

mod loader_package;
mod tools;
mod user_data;

pub use loader_package::LoaderPackage;
pub use tools::{LogKind, LogSource, Prerequisite, Step, Tool};
pub use user_data::{Detect, Executable, LoadOrder, OrderFormat, UserData, Verified};

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::domain::deploy::Deploy;
use crate::domain::mod_steps::Phase;
use crate::domain::user_data::Isolation;

/// The newest `game.toml` schema this app's parser understands. A definition with a higher schema is refused, and the
/// catalog blocks it until the app is updated, so an older app never tries to read a file it cannot interpret.
pub const SUPPORTED_SCHEMA: u32 = 1;

/// One game definition: the rules read from its `game.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameDef {
    pub schema: u32,
    pub id: String,
    pub name: String,
    /// The oldest Gobbo release that serves this definition correctly, such as `"0.3.0"`. Set it when the file
    /// depends on a fix or a feature an older app lacks even though it would still parse there.
    #[serde(default)]
    pub min_gobbo: Option<String>,
    /// A real installation was tested: the date, the systems and the versions. Absent on a game that is not verified.
    #[serde(default)]
    pub verified: Option<Verified>,
    /// The external mod providers this game uses, by provider id. A provider without an entry is not offered for the
    /// game.
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderGame>,
    /// A shared loader preset whose `requires` and `[deploy]` rules are merged under this game's own.
    #[serde(default)]
    pub loader: Option<LoaderRef>,
    /// The loader preset merged into this game, when it names one. Filled by `loaders::apply` for the interface.
    #[serde(skip)]
    pub resolved_loader: Option<ResolvedLoader>,
    pub detect: Detect,
    /// The programs the game can be started with. The first one is the default.
    #[serde(default)]
    pub executables: Vec<Executable>,
    #[serde(default)]
    pub deploy: Deploy,
    /// The file the game reads its mod order from. Without it the game orders its mods itself.
    #[serde(default)]
    pub load_order: Option<LoadOrder>,
    /// Files and folders that belong to one profile, for example saves or per-profile settings (path templates).
    /// Kept for simple copy-only entries; `user_data` is the folder-aware, optionally redirected primitive.
    #[serde(default)]
    pub profile_data: Vec<String>,
    /// Folders outside the game that the game treats as its own user data. Each one can be shared or isolated per
    /// profile, and isolated sets can be served by the overlay instead of copied.
    #[serde(default)]
    pub user_data: Vec<UserData>,
    /// Files that must exist in the game folder before mods can be deployed, for example a mod loader's core file. Use
    /// it for a file with no display name or page; a file in `requirements` is required without being listed here.
    /// After the merge, this holds every `requirements` path too (see `loaders::apply`).
    #[serde(default)]
    pub requires: Vec<String>,
    /// Files the game needs, each with a display name and a page to get it, for the setup check. A requirement implies
    /// its `path`: the file is required whether or not `requires` lists it.
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    /// The loader package the game can install into its folder. A loader preset carries the shared package; the game
    /// adds assets when it needs one more file.
    #[serde(default)]
    pub install: Option<LoaderPackage>,
    #[serde(default)]
    pub tools: Vec<Tool>,
    /// Programs the game needs run in its overlay, before it starts or after it exits. A mod asks for them by id in
    /// `mod_steps.toml`; the code never knows a program by name.
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Log files the game itself writes, shown in the console after a launch. Mod `.log` files are discovered in the
    /// mod and overwrite layers without a definition entry. A path is a file or a folder; a folder is searched for
    /// files whose name matches `pattern`, and the newest `limit` are shown.
    #[serde(default)]
    pub logs: Vec<LogSource>,
}

/// A file a game or its loader needs, with the name the setup check shows and the page where the user gets it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    /// The name the user knows, for example `BepInEx`.
    pub name: String,
    /// The file or folder that must exist in the game folder, relative to it.
    pub path: String,
    /// The page where the user downloads it. Without one the setup check only says what is missing.
    #[serde(default)]
    pub url: Option<String>,
}

/// A reference to a shared loader preset: `loader = "bepinex"` or `loader = { id = "bepinex", variant = "6" }`. The
/// string form uses the preset's `default_variant`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderRef {
    id: String,
    variant: Option<String>,
}

impl LoaderRef {
    /// The preset id, the name of its file in `loaders/` without `.toml`.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The preset variant, or `None` for the preset's default.
    pub fn variant(&self) -> Option<&str> {
        self.variant.as_deref()
    }
}

impl<'de> Deserialize<'de> for LoaderRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct LoaderRefVisitor;

        impl<'de> serde::de::Visitor<'de> for LoaderRefVisitor {
            type Value = LoaderRef;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a loader id or a table with `id` and optional `variant`")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(LoaderRef {
                    id: value.to_string(),
                    variant: None,
                })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut id = None;
                let mut variant = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => {
                            if id.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }

                            id = Some(map.next_value::<String>()?);
                        }
                        "variant" => {
                            if variant.is_some() {
                                return Err(serde::de::Error::duplicate_field("variant"));
                            }

                            variant = Some(map.next_value::<String>()?);
                        }
                        other => {
                            return Err(serde::de::Error::unknown_field(other, &["id", "variant"]));
                        }
                    }
                }

                let id = id.ok_or_else(|| serde::de::Error::missing_field("id"))?;
                Ok(LoaderRef { id, variant })
            }
        }

        deserializer.deserialize_any(LoaderRefVisitor)
    }
}

/// The loader preset a game uses after the preset was read and its variant chosen. Not part of `game.toml`; the
/// interface shows it.
#[derive(Debug, Clone)]
pub struct ResolvedLoader {
    pub id: String,
    pub name: String,
    /// The game-relative folder an unpacked community profile keeps this loader's config files in, when it has one.
    pub config_mirror: Option<String>,
    pub variant: Option<String>,
    /// Whether the variant came from the preset's `default_variant`, not from the game's line.
    pub default_variant: bool,
}

/// The game's entry on an external mod provider. The key is what the provider needs to address the game: a Nexus
/// domain, a mod.io game id, and so on.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderGame {
    /// The provider's key for this game. A provider entry without one cannot name the game and is refused.
    #[serde(default)]
    pub key: Option<String>,
}

impl GameDef {
    /// The step the game defines with `id` for `phase`, or `None` when it defines no such step.
    pub fn step(&self, id: &str, phase: Phase) -> Option<&Step> {
        self.steps
            .iter()
            .find(|step| step.id == id && step.phase == phase)
    }

    /// The key the provider uses for this game, when the game is on that provider.
    pub fn provider_key(&self, provider: &str) -> Option<&str> {
        self.providers
            .get(provider)
            .and_then(|game| game.key.as_deref())
    }

    /// The default isolation of every user-data set, for a new profile.
    pub fn user_data_defaults(&self) -> BTreeMap<String, Isolation> {
        self.user_data
            .iter()
            .map(|set| (set.id.clone(), set.default))
            .collect()
    }
}
