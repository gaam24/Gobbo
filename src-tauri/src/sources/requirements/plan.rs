//! The requirement planner: the recursive walk, the alternatives and the cycles.

use std::collections::BTreeSet;

use crate::context::{Data, load_library, load_profiles};
use crate::domain::library::Library;
use crate::domain::library::LibraryView;
use crate::domain::naming::requirement_label;
use crate::domain::source::{SourceRef, SourceRequirement};
use crate::error::{AppError, Message};
use crate::games::{self, GameDef};
use crate::library::mods_of;
use crate::library::relations::{RequirementState, SourceLookup};
use crate::providers;

use super::model::{RequirementAction, RequirementPlan, RequirementStep};

/// The plan for the mod whose selected version is `folder`. Only requirements the library already knows are walked;
/// a mod that is not installed yet can add more requirements once it arrives, so the preview is run again after a
/// bulk install.
pub fn plan(root: &Data, game_id: &str, folder: &str) -> Result<RequirementPlan, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = load_library(&data, &game)?;
    let view = LibraryView::new(&library);
    let profiles = load_profiles(&data)?;
    let Some(version) = library.versions.get(folder) else {
        return Err(AppError::user("requirements.unknown").with("folder", folder));
    };
    let root_family = version.family.clone();
    // The same profile list and lookup the status uses, so the planner and the status agree on what is on.
    let listed = mods_of(profiles.active(), &library, &game);
    let lookup = SourceLookup::new(&view, &listed);
    let mut builder = Builder {
        library: &library,
        view: &view,
        game: &game,
        lookup,
        visiting: Vec::new(),
        visited: BTreeSet::new(),
        steps: Vec::new(),
        seen: BTreeSet::new(),
        cycles: Vec::new(),
    };
    builder.walk(&root_family, 0);
    let enable = count(&builder.steps, RequirementAction::Enable);
    let download = count(&builder.steps, RequirementAction::Download);
    let blocked = count(&builder.steps, RequirementAction::Blocked);
    Ok(RequirementPlan {
        root: builder.name(&root_family),
        steps: builder.steps,
        cycles: builder.cycles,
        enable,
        download,
        blocked,
    })
}

fn count(steps: &[RequirementStep], action: RequirementAction) -> u32 {
    u32::try_from(steps.iter().filter(|step| step.action == action).count()).unwrap_or(u32::MAX)
}

struct Builder<'a> {
    library: &'a Library,
    view: &'a LibraryView<'a>,
    game: &'a GameDef,
    /// The enabled families of the active profile and the state of each requirement, the same rule the status uses.
    lookup: SourceLookup<'a, 'a, 'a>,
    /// The chain being walked, for a cycle message.
    visiting: Vec<String>,
    /// Families whose requirements were already added.
    visited: BTreeSet<String>,
    steps: Vec<RequirementStep>,
    seen: BTreeSet<String>,
    cycles: Vec<Message>,
}

impl Builder<'_> {
    fn walk(&mut self, family_id: &str, depth: u32) {
        if let Some(at) = self.visiting.iter().position(|id| id == family_id) {
            let path = self.visiting[at..]
                .iter()
                .chain(std::iter::once(&family_id.to_string()))
                .map(|id| self.name(id))
                .collect::<Vec<_>>()
                .join(" → ");
            self.cycles.push(
                Message::new("requirements.cycle")
                    .with("path", path)
                    .with("depth", depth),
            );
            return;
        }

        if !self.visited.insert(family_id.to_string()) {
            return;
        }

        let Some(family) = self.library.families.get(family_id) else {
            return;
        };
        self.visiting.push(family_id.to_string());

        for required in &family.requires {
            if self.visiting.iter().any(|id| id == required) {
                self.walk(required, depth + 1);
                continue;
            }

            let step = self.required_step(required, depth + 1);
            self.push(step);

            if self.library.families.contains_key(required) {
                self.walk(required, depth + 1);
            }
        }

        for (at, alternatives) in
            crate::library::relations::source_alternatives(&family.requirements)
                .into_iter()
                .enumerate()
        {
            let alternatives: Vec<&SourceRequirement> = alternatives
                .into_iter()
                .filter(|requirement| !requirement.ignored)
                .collect();
            if alternatives.is_empty() {
                continue;
            }

            let group = (alternatives.len() > 1).then(|| format!("one-of:{family_id}:{at}"));
            let step = self.alternative_step(&alternatives, depth + 1, group);
            let family = step.family.clone();

            if let Some(family) = &family
                && self.visiting.iter().any(|id| id == family)
            {
                self.walk(family, depth + 1);
                continue;
            }

            self.push(step);

            if let Some(family) = family {
                self.walk(&family, depth + 1);
            }
        }

        self.visiting.pop();
    }

    fn required_step(&self, required: &str, depth: u32) -> RequirementStep {
        let name = self.name(required);

        if !self.library.families.contains_key(required) {
            return RequirementStep {
                id: format!("local:{required}"),
                family: None,
                source: None,
                name: name.clone(),
                depth,
                group: None,
                action: RequirementAction::Blocked,
                message: Some(Message::new(crate::error::MOD_MISSING).with("folder", name)),
            };
        }

        let action = if self.lookup.is_enabled(required) {
            RequirementAction::AlreadyOn
        } else {
            RequirementAction::Enable
        };
        RequirementStep {
            id: format!("family:{required}"),
            family: Some(required.to_string()),
            source: None,
            name,
            depth,
            group: None,
            action,
            message: None,
        }
    }

    fn alternative_step(
        &self,
        alternatives: &[&SourceRequirement],
        depth: u32,
        group: Option<String>,
    ) -> RequirementStep {
        let enabled = alternatives
            .iter()
            .find(|requirement| self.state(requirement) == RequirementAction::AlreadyOn);
        if let Some(requirement) = enabled {
            return self.step(requirement, depth, group, RequirementAction::AlreadyOn);
        }

        let disabled = alternatives
            .iter()
            .find(|requirement| self.state(requirement) == RequirementAction::Enable);
        if let Some(requirement) = disabled {
            return self.step(requirement, depth, group, RequirementAction::Enable);
        }
        // Every alternative is missing from the library. Prefer one that Gobbo can download now; when none can, keep
        // the first and let the plan say what is missing.
        let downloadable = alternatives
            .iter()
            .find(|requirement| self.blocker(&requirement.source).is_none());
        let requirement = downloadable.unwrap_or(&alternatives[0]);
        let action = if downloadable.is_some() {
            RequirementAction::Download
        } else {
            RequirementAction::Blocked
        };
        self.step(requirement, depth, group, action)
    }

    fn step(
        &self,
        requirement: &SourceRequirement,
        depth: u32,
        group: Option<String>,
        action: RequirementAction,
    ) -> RequirementStep {
        let source = requirement.source.clone();
        let family = self
            .view
            .family_of_source(&source.provider, &source.mod_id)
            .map(str::to_string);
        let name = requirement_label(
            &requirement.name,
            requirement.source.provider.as_str(),
            &requirement.source.mod_id,
        );
        let message = if action == RequirementAction::Blocked {
            self.blocker(&source)
        } else {
            None
        };
        RequirementStep {
            id: format!("source:{}:{}", source.provider, source.mod_id),
            family,
            source: Some(source),
            name,
            depth,
            group,
            action,
            message,
        }
    }

    /// The action a requirement's state calls for: on already, switched off so it can be enabled, or missing so it
    /// has to be downloaded.
    fn state(&self, requirement: &SourceRequirement) -> RequirementAction {
        match self.lookup.state_source(&requirement.source) {
            RequirementState::Enabled => RequirementAction::AlreadyOn,
            RequirementState::Disabled => RequirementAction::Enable,
            RequirementState::Missing => RequirementAction::Download,
        }
    }

    /// Why a provider requirement cannot be downloaded now, or `None` when it can. A requirement in the library never
    /// reaches this check. The download route is the one decision for the account: its browser error keeps the code for
    /// a missing capability or key and reports `payment_required` for a saved account without direct downloads.
    fn blocker(&self, source: &SourceRef) -> Option<Message> {
        let provider = match providers::require(source.provider.as_str()) {
            Ok(provider) => provider,
            Err(error) => return Some(error.into()),
        };

        if let Some(error) = providers::browser_route_error(provider) {
            return Some(error.into());
        }

        if self.game.provider_key(provider.id().as_str()).is_none() {
            return Some(providers::no_game_key(provider.id().as_str()).into());
        }

        None
    }

    fn push(&mut self, step: RequirementStep) {
        if self.seen.insert(step.id.clone()) {
            self.steps.push(step);
        }
    }

    fn name(&self, family: &str) -> String {
        self.library
            .families
            .get(family)
            .map_or_else(|| family.to_string(), |family| family.name.clone())
    }
}
