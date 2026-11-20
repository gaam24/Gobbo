//! The backend-owned browser step of **Update all**. A provider that cannot download directly needs the user to visit
//! its files page and download each file by hand. This module owns that sequence: which target is current, the state of
//! every target, and the one undo handoff. The window renders a [`ManualUpdateView`], opens the page the controller asks
//! for, and reports a finished download with a command. All of it lives in the backend, so a restarted window re-reads
//! the same run instead of losing it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::UpdateChange;

/// The event that asks the window to open a provider page for one target of a browser run. The backend never builds a
/// URL the window has to trust; it names the mod and the window uses its normal page action.
pub const MANUAL_PAGE_EVENT: &str = "manual-update-open-page";

/// The page request carried by [`MANUAL_PAGE_EVENT`] and returned by the reopen command.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ManualPageRequest {
    pub game: String,
    pub id: String,
    pub name: String,
    pub provider: String,
}

/// One mod of a browser run. The window sends the fields it already has from its update report, so the backend can
/// select and undo without asking the window to keep any of the sequence state.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub struct ManualTarget {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub mod_id: String,
    pub folder: String,
    pub current: Option<String>,
    pub latest: Option<String>,
    /// The newest version is installed already; the run only selects it.
    pub installed: bool,
}

/// The state of one target. `downloading` and `installing` are reported by the window while a download is in flight;
/// the controller's own transitions are `waiting`, `selected`, `skipped` and `failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ManualStep {
    Waiting,
    Downloading,
    Installing,
    Selecting,
    Selected,
    Failed,
    Skipped,
}

/// The view the window renders for one browser run.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ManualUpdateView {
    pub game: String,
    pub active: bool,
    pub targets: Vec<ManualTarget>,
    /// The one-based place of the current target, as a portable IPC number.
    pub index: u32,
    pub steps: BTreeMap<String, ManualStep>,
    pub errors: BTreeMap<String, String>,
    pub percent: u32,
    pub current: Option<ManualPageRequest>,
}

/// What a finished browser run hands back for the single undo: the versions it selected and the failures it kept.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ManualUpdateFinish {
    pub game: String,
    pub changes: Vec<UpdateChange>,
    pub failures: Vec<String>,
}

/// The controller's state for one game. It is plain data: a fresh registry can take a snapshot of it and carry on,
/// which is the restart case the tests cover.
#[derive(Debug, Clone)]
pub struct ManualSession {
    game: String,
    targets: Vec<ManualTarget>,
    index: usize,
    steps: BTreeMap<String, ManualStep>,
    errors: BTreeMap<String, String>,
    changes: Vec<UpdateChange>,
    advanced: BTreeSet<String>,
}

impl ManualSession {
    fn new(game: String, targets: Vec<ManualTarget>) -> Self {
        let steps = targets
            .iter()
            .map(|target| (target.id.clone(), ManualStep::Waiting))
            .collect();
        Self {
            game,
            targets,
            index: 0,
            steps,
            errors: BTreeMap::new(),
            changes: Vec::new(),
            advanced: BTreeSet::new(),
        }
    }

    fn current(&self) -> Option<&ManualTarget> {
        self.targets.get(self.index)
    }

    fn page(&self) -> Option<ManualPageRequest> {
        self.current().map(|target| ManualPageRequest {
            game: self.game.clone(),
            id: target.id.clone(),
            name: target.name.clone(),
            provider: target.provider.clone(),
        })
    }

    fn view(&self) -> ManualUpdateView {
        let done = self
            .targets
            .iter()
            .filter(|target| {
                matches!(
                    self.steps.get(&target.id),
                    Some(ManualStep::Selected | ManualStep::Skipped)
                )
            })
            .count();
        let percent = if self.targets.is_empty() {
            100
        } else {
            crate::domain::progress::percent(done as u64, self.targets.len() as u64).unwrap_or(100)
        };
        ManualUpdateView {
            game: self.game.clone(),
            active: true,
            targets: self.targets.clone(),
            index: u32::try_from(self.index).unwrap_or(u32::MAX),
            steps: self.steps.clone(),
            errors: self.errors.clone(),
            percent,
            current: self.page(),
        }
    }

    /// Marks one target settled and moves to the next target that still needs work. Terminal targets are never
    /// revisited, so a report of the same download cannot advance the run twice.
    fn settle(
        &mut self,
        id: &str,
        step: ManualStep,
        change: Option<UpdateChange>,
        error: Option<String>,
    ) {
        if self.advanced.contains(id) && step != ManualStep::Failed {
            return;
        }

        self.steps.insert(id.to_string(), step);

        if let Some(change) = change
            && !self.changes.iter().any(|known| known.id == change.id)
        {
            self.changes.push(change);
        }

        if let Some(error) = error {
            self.errors.insert(id.to_string(), error);
        } else {
            self.errors.remove(id);
        }

        self.advanced.insert(id.to_string());
        self.move_to_next();
    }

    fn move_to_next(&mut self) {
        if self.index >= self.targets.len() {
            return;
        }

        let next = self.targets[self.index + 1..]
            .iter()
            .position(|target| !self.advanced.contains(&target.id));
        self.index = match next {
            Some(offset) => self.index + 1 + offset,
            None => self.targets.len(),
        };
    }

    fn step(&mut self, id: &str, step: ManualStep) {
        if self.advanced.contains(id) {
            return;
        }

        self.steps.insert(id.to_string(), step);
    }

    fn finish(&self) -> ManualUpdateFinish {
        let failures = self
            .targets
            .iter()
            .filter(|target| self.steps.get(&target.id) == Some(&ManualStep::Failed))
            .map(|target| {
                self.errors
                    .get(&target.id)
                    .cloned()
                    .unwrap_or_else(|| target.id.clone())
            })
            .collect();
        ManualUpdateFinish {
            game: self.game.clone(),
            changes: self.changes.clone(),
            failures,
        }
    }
}

/// The running browser sequences of every game, managed by the app. This is app-managed state, not a global static: a
/// registry is injected by Tauri and can be replaced wholesale in a test to simulate a controller restart.
#[derive(Default)]
pub struct ManualUpdateSessions {
    inner: Mutex<BTreeMap<String, ManualSession>>,
}

impl ManualUpdateSessions {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, ManualSession>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a run, replacing any earlier run of the same game. The first page request is returned so the command can
    /// ask the window to open it.
    pub fn begin(&self, game: String, targets: Vec<ManualTarget>) -> ManualUpdateView {
        let session = ManualSession::new(game.clone(), targets);
        let view = session.view();
        self.lock().insert(game, session);
        view
    }

    /// The current view, or an inactive one when the game has no run.
    pub fn view(&self, game: &str) -> ManualUpdateView {
        self.lock().get(game).map_or_else(
            || ManualUpdateView {
                game: game.to_string(),
                active: false,
                targets: Vec::new(),
                index: 0,
                steps: BTreeMap::new(),
                errors: BTreeMap::new(),
                percent: 0,
                current: None,
            },
            ManualSession::view,
        )
    }

    /// Records a transient step for a target that has not settled yet.
    pub fn step(&self, game: &str, id: &str, step: ManualStep) -> ManualUpdateView {
        if let Some(session) = self.lock().get_mut(game) {
            session.step(id, step);
        }

        self.view(game)
    }

    /// Marks the current or named target selected after its download imported and the backend selected the version.
    /// Returns the next page request, if any, so the command can open it.
    pub fn selected(
        &self,
        game: &str,
        id: &str,
        change: UpdateChange,
    ) -> (ManualUpdateView, Option<ManualPageRequest>) {
        let mut inner = self.lock();
        let Some(session) = inner.get_mut(game) else {
            drop(inner);
            return (self.view(game), None);
        };
        session.settle(id, ManualStep::Selected, Some(change), None);
        let page = session.page();
        let view = session.view();
        drop(inner);
        (view, page)
    }

    /// Skips the target the run is waiting on.
    pub fn skip(&self, game: &str) -> (ManualUpdateView, Option<ManualPageRequest>) {
        let mut inner = self.lock();
        let Some(session) = inner.get_mut(game) else {
            drop(inner);
            return (self.view(game), None);
        };
        let Some(id) = session.current().map(|target| target.id.clone()) else {
            let view = session.view();
            drop(inner);
            return (view, None);
        };
        session.settle(&id, ManualStep::Skipped, None, None);
        let page = session.page();
        let view = session.view();
        drop(inner);
        (view, page)
    }

    /// Records a failure for the target and keeps it on screen with its message.
    pub fn failed(&self, game: &str, id: &str, message: String) -> ManualUpdateView {
        if let Some(session) = self.lock().get_mut(game) {
            session.settle(id, ManualStep::Failed, None, Some(message));
        }

        self.view(game)
    }

    /// Takes the run off the registry and hands its changes and failures to the caller for one undo.
    pub fn finish(&self, game: &str) -> Option<ManualUpdateFinish> {
        self.lock().remove(game).map(|session| session.finish())
    }

    /// Stops a run without a finish handoff. Downloads already started import themselves, as before.
    pub fn cancel(&self, game: &str) {
        self.lock().remove(game);
    }

    /// The page request to reopen the current step after a game switch.
    pub fn reopen(&self, game: &str) -> Option<ManualPageRequest> {
        self.lock().get(game).and_then(ManualSession::page)
    }

    /// A copy of the run's state, used by tests to simulate a controller restart.
    #[cfg(test)]
    pub fn snapshot(&self, game: &str) -> Option<ManualSession> {
        self.lock().get(game).cloned()
    }

    /// Restores a snapshot into a fresh registry, used by tests to simulate a controller restart.
    #[cfg(test)]
    pub fn restore(&self, game: String, session: ManualSession) {
        self.lock().insert(game, session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: &str) -> ManualTarget {
        ManualTarget {
            id: id.to_string(),
            name: format!("Mod {id}"),
            provider: "test-provider".into(),
            mod_id: format!("mod-{id}"),
            folder: format!("folder-{id}"),
            current: Some("1.0".into()),
            latest: Some("2.0".into()),
            installed: false,
        }
    }

    fn change(id: &str) -> UpdateChange {
        UpdateChange {
            id: id.to_string(),
            name: format!("Mod {id}"),
            previous: format!("folder-{id}"),
        }
    }

    #[test]
    fn a_run_opens_the_first_page_and_advances_on_a_finished_download() {
        let sessions = ManualUpdateSessions::default();
        let view = sessions.begin("game".into(), vec![target("a"), target("b")]);
        assert_eq!(view.index, 0);
        assert_eq!(
            view.current.as_ref().map(|page| page.id.as_str()),
            Some("a")
        );

        let (view, page) = sessions.selected("game", "a", change("a"));
        assert_eq!(view.index, 1);
        assert_eq!(view.steps.get("a"), Some(&ManualStep::Selected));
        assert_eq!(view.percent, 50);
        assert_eq!(page.map(|page| page.id), Some("b".to_string()));
    }

    #[test]
    fn a_sequence_survives_a_controller_restart() {
        let first = ManualUpdateSessions::default();
        first.begin("game".into(), vec![target("a"), target("b"), target("c")]);
        first.selected("game", "a", change("a"));
        let snapshot = first.snapshot("game").expect("the run exists");

        // The process restarts: a fresh registry takes over from the snapshot of the same run.
        let restarted = ManualUpdateSessions::default();
        restarted.restore("game".into(), snapshot);
        let view = restarted.view("game");
        assert_eq!(view.index, 1);
        assert_eq!(view.percent, 33);
        assert_eq!(
            view.current.as_ref().map(|page| page.id.as_str()),
            Some("b")
        );

        let (view, _) = restarted.skip("game");
        assert_eq!(view.index, 2);
        let (_, page) = restarted.selected("game", "c", change("c"));
        assert!(page.is_none());
        let handoff = restarted.finish("game").expect("the run exists");
        assert_eq!(handoff.changes.len(), 2);
        assert!(handoff.failures.is_empty());
    }

    #[test]
    fn a_failure_stays_in_the_handoff_and_does_not_advance_twice() {
        let sessions = ManualUpdateSessions::default();
        sessions.begin("game".into(), vec![target("a"), target("b")]);
        sessions.failed("game", "a", "Download failed".into());
        sessions.selected("game", "a", change("a"));
        let view = sessions.view("game");
        assert_eq!(view.index, 1);
        let handoff = sessions.finish("game").expect("the run exists");
        assert!(handoff.changes.is_empty());
        assert_eq!(handoff.failures, ["Download failed"]);
    }

    #[test]
    fn a_missing_run_is_inactive_and_finishes_nothing() {
        let sessions = ManualUpdateSessions::default();
        assert!(!sessions.view("game").active);
        assert!(sessions.finish("game").is_none());
    }
}
