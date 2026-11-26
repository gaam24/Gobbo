//! The per-file rules that decide which files a summary must materialize.

use crate::domain::deploy::Deploy;
use crate::domain::layout::{LayoutProfile, Materialized};
use crate::domain::resolver::shared::SharedRules;

/// The files a summary of this list has to materialize, under the game's mirror, root-file and shared rules. The
/// library computes this once when it loads; a status then skips the per-file rules for most mods.
pub fn layout_profile(deploy: &Deploy, files: &[String]) -> LayoutProfile {
    LayoutRules::new(deploy).profile(files)
}

/// The game rules of a layout, prepared once for many file lists. Building `SharedRules` per version was an allocation
/// per version on a large library load.
pub struct LayoutRules<'a> {
    deploy: &'a Deploy,
    shared: SharedRules,
}

impl<'a> LayoutRules<'a> {
    /// Prepares the layout rules of `deploy` once, for many file lists.
    pub fn new(deploy: &'a Deploy) -> Self {
        Self {
            deploy,
            shared: SharedRules::new(&deploy.shared),
        }
    }

    /// The files of `files` a summary must materialize under the prepared rules.
    pub fn profile(&self, files: &[String]) -> LayoutProfile {
        let mirrored_only = self.deploy.mirrored_only(files);

        if mirrored_only {
            return LayoutProfile {
                materialized: Materialized::All,
                mirrored_only: true,
            };
        }

        let mut indices = Vec::new();

        for (index, rel) in files.iter().enumerate() {
            if self.deploy.keeps_game_path(rel) || self.shared.matches(rel) {
                indices.push(u32::try_from(index).unwrap_or(u32::MAX));
            }
        }

        let materialized = if files.is_empty() || indices.is_empty() {
            Materialized::None
        } else if indices.len() == files.len() {
            Materialized::All
        } else {
            Materialized::Some(indices)
        };
        LayoutProfile {
            materialized,
            mirrored_only: false,
        }
    }
}
