//! Which steps each mod needs, read from `mod_steps.toml` in the game's data folder. The file is keyed by the mod's
//! folder in the library. A mod names steps the game defines (`[[steps]]` in `game.toml`); it never names a program,
//! so a mod file cannot run anything the game definition does not allow.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// When a step runs: before the game starts, or after it has exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    BeforeStart,
    AfterEnd,
}

/// The contents of `mod_steps.toml`.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ModSteps {
    #[serde(default)]
    pub mods: BTreeMap<String, Phases>,
}

/// The step ids a mod asks for in each phase, in the order they run.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Phases {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before_start: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after_end: Vec<String>,
}

impl ModSteps {
    /// Reads the text of a `mod_steps.toml` file, or an error when it is malformed.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// The file's text. Comments the file had are not kept.
    pub fn to_text(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }
}

/// The step ids to run in `phase`: the enabled mods in profile order, and within a mod, the order of its list. A
/// step that several mods ask for runs once, at its first place.
pub fn plan(steps: &ModSteps, enabled_mods: &[String], phase: Phase) -> Vec<String> {
    let mut planned: Vec<String> = Vec::new();

    for folder in enabled_mods {
        let Some(phases) = steps.mods.get(folder) else {
            continue;
        };
        let ids = match phase {
            Phase::BeforeStart => &phases.before_start,
            Phase::AfterEnd => &phases.after_end,
        };

        for id in ids {
            if !planned.contains(id) {
                planned.push(id.clone());
            }
        }
    }

    planned
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
[mods."modA-1.0"]
before_start = ["compile", "patch"]
after_end = ["cleanup"]

[mods."modB-2.0"]
before_start = ["patch"]
"#;

    fn folders(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn steps_are_planned_by_mod_in_profile_order_once_each() {
        let steps = ModSteps::parse(FILE).unwrap();
        let planned = plan(
            &steps,
            &folders(&["modB-2.0", "modA-1.0"]),
            Phase::BeforeStart,
        );
        assert_eq!(planned, ["patch", "compile"]);
    }

    #[test]
    fn a_disabled_mod_asks_for_nothing() {
        let steps = ModSteps::parse(FILE).unwrap();
        assert!(plan(&steps, &folders(&["modB-2.0"]), Phase::AfterEnd).is_empty());
        assert_eq!(
            plan(&steps, &folders(&["modA-1.0"]), Phase::AfterEnd),
            ["cleanup"]
        );
    }
}
