//! The backend-owned controller of a collection install. A round imports every member that needs no decision and then
//! pauses with a prompt descriptor; the window only renders that descriptor and sends one decision back. The controller
//! keeps no loop state of its own: the round's progress lives in the persisted session and the cancellation flag, so a
//! restarted window resumes from the same place.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::download;
use super::model::{
    CollectionInstallView, CollectionItemAction, CollectionProgress, CollectionSessionView,
};
use super::session;
use crate::context::Data;
use crate::domain::collection::CollectionSource;
use crate::domain::fomod::Choice;
use crate::error::AppError;
use crate::import::model::FomodStepView;
use crate::sources::downloads::Downloads;

/// The code `download::import_file` uses when a hand-picked archive turns out to be a FOMOD installer. The wizard has
/// to run before the member can be imported; it is not a failure of the collection.
const HAND_PICKED_NEEDS_CHOICE: &str = "collection.needs_choice";

/// The event that asks the window to open a provider page for a member of an automatic round whose file the user must
/// pick by hand. The backend owns the page URL and the download expectation; the window only opens the page.
pub const COLLECTION_PAGE_EVENT: &str = "collection-open-page";

/// The page request carried by [`COLLECTION_PAGE_EVENT`] and by a round that is waiting on a member. The window opens
/// the page through its normal source action; it never has to build a URL or guess the download's owner.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CollectionPageRequest {
    pub game: String,
    pub profile: String,
    pub collection: String,
    pub item: String,
    pub name: String,
    pub provider: String,
    pub game_key: String,
    pub mod_id: String,
    pub url: String,
}

/// The one decision the window must make before a round can continue. It is derived from the round and echoed back
/// with the chosen answer; the controller never asks the window to pick the next member or to drive the loop.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionPrompt {
    /// A FOMOD archive has no stored choices; the shared wizard must supply them.
    Choice {
        item: String,
        name: String,
        path: String,
        module_name: String,
        steps: Vec<FomodStepView>,
    },
    /// A manual or browse member needs a picked file, a "mark done" or a skip.
    Manual {
        item: String,
        name: String,
        url: Option<String>,
        instructions: String,
    },
}

/// The answer to a [`CollectionPrompt`]. `item` names the member the prompt referred to; it is an identifier the
/// window echoes, not loop state. The archive `path` is the one the prompt named or the file the user picked.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionDecision {
    /// Import the prompted archive with the wizard's FOMOD choices.
    Choice { path: String, choices: Vec<Choice> },
    /// Import a file the user picked for a manual or waiting member.
    File { path: String },
    /// Mark a manual or browse member installed by hand.
    MarkDone,
    /// Skip the member, permanently when `permanent` (then the profile origin excludes it).
    Skip { permanent: bool },
}

/// What one backend-driven round did. `install` carries the fresh session and the round's notices; `prompt` is the
/// single decision that pauses the loop, `waiting` is a member whose provider page is open and whose owned download is
/// expected, and `finished` says the round has no pending members left. A waiting round needs no user decision: the
/// backend imports the matching download and continues on its own.
#[derive(Debug, Serialize, Type)]
pub struct CollectionRoundView {
    pub install: CollectionInstallView,
    pub prompt: Option<CollectionPrompt>,
    pub waiting: Option<CollectionPageRequest>,
    pub finished: bool,
}

/// The pure part of a round: which decision the outcome needs and whether the round is finished. Plain data in, plain
/// data out, so the pause and resume rules are testable without a game, a library or a provider.
#[derive(Debug, Clone)]
pub struct RoundState {
    pub prompt: Option<CollectionPrompt>,
    pub finished: bool,
}

/// Reads the decision a round outcome needs. A FOMOD choice wins over a manual file, which wins over a waiting
/// download: the controller always asks about one member at a time and the same round can contain all three kinds.
pub fn classify(view: &CollectionInstallView) -> RoundState {
    let prompt = view
        .needs_choice
        .first()
        .map(|choice| CollectionPrompt::Choice {
            item: choice.key.clone(),
            name: choice.name.clone(),
            path: choice.path.clone(),
            module_name: choice.module_name.clone(),
            steps: choice.steps.clone(),
        })
        .or_else(|| {
            view.needs_user.first().and_then(|key| {
                let item = view.session.plan.item(key)?;
                Some(CollectionPrompt::Manual {
                    item: key.clone(),
                    name: item.name.clone(),
                    url: match &item.source {
                        crate::domain::collection::CollectionSource::Browse { url } => {
                            Some(url.clone())
                        }
                        _ => None,
                    },
                    instructions: item.instructions.clone(),
                })
            })
        });
    let finished = view.session.counts.pending == 0
        && view.needs_choice.is_empty()
        && view.needs_user.is_empty();
    RoundState { prompt, finished }
}

/// Wraps a session with an empty round outcome. Used when a decision itself settles a member (mark done or skip) and
/// no download happens, so the window still receives a complete round view.
fn settled(session: CollectionSessionView) -> CollectionInstallView {
    CollectionInstallView {
        profile: session.profile.clone(),
        installed: Vec::new(),
        failed: Vec::new(),
        needs_choice: Vec::new(),
        needs_user: Vec::new(),
        skipped: Vec::new(),
        warnings: Vec::new(),
        cancelled: false,
        session,
    }
}

fn view(
    root: &Data,
    game: &str,
    profile: &str,
    collection: &str,
) -> Result<CollectionInstallView, AppError> {
    Ok(settled(session::status(root, game, profile, collection)?))
}

/// Drives one full automatic round: every member that needs no decision is imported, then the fresh session and the
/// first prompt are returned. Repeating rounds happens here until a prompt, a waiting download or completion; the
/// window calls this once per decision, never in a loop.
pub fn run_round(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    downloads: &Downloads,
    progress: &mut dyn FnMut(CollectionProgress),
    cancel: Option<&AtomicBool>,
) -> Result<CollectionRoundView, AppError> {
    let install =
        download::install_automatic(root, game_id, profile, collection, progress, cancel)?;
    finish_round(root, game_id, profile, collection, downloads, install)
}

/// Turns an automatic install into the round the window renders. A member whose provider file has to be picked from a
/// web page is not offered as a manual decision: its expectation is registered, the first page request is returned for
/// the window to open, and the round waits for the owned download. The explicit pick-file / mark-done / skip prompt is
/// still the path for members that need no provider page.
fn finish_round(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    downloads: &Downloads,
    mut install: CollectionInstallView,
) -> Result<CollectionRoundView, AppError> {
    let pages = page_requests(root, game_id, profile, collection, &install, downloads)?;
    let waiting = pages.first().cloned();

    if !pages.is_empty() {
        let waiting_keys: BTreeSet<&str> = pages.iter().map(|page| page.item.as_str()).collect();
        install
            .needs_user
            .retain(|key| !waiting_keys.contains(key.as_str()));
    }

    let mut round = round_view(install);
    round.waiting = waiting;
    Ok(round)
}

/// Registers the download expectation of every member of `view` whose provider needs a web page and returns the page
/// request for each one. The window opens one page at a time; the other expectations are already in place so downloads
/// that arrive in a different order are still owned by their member.
fn page_requests(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    view: &CollectionInstallView,
    downloads: &Downloads,
) -> Result<Vec<CollectionPageRequest>, AppError> {
    let mut pages = Vec::new();

    for key in &view.needs_user {
        let Some(item) = view.session.plan.item(key) else {
            continue;
        };
        let CollectionSource::Provider {
            provider, mod_id, ..
        } = &item.source
        else {
            continue;
        };
        let Some(game_key) = crate::games::load(game_id)?
            .provider_key(provider)
            .map(str::to_string)
        else {
            continue;
        };
        let action = download::prepare_item(root, game_id, profile, collection, key, downloads)?;
        let CollectionItemAction::Waiting { url, .. } = action else {
            continue;
        };
        pages.push(CollectionPageRequest {
            game: game_id.to_string(),
            profile: profile.to_string(),
            collection: collection.to_string(),
            item: key.clone(),
            name: item.name.clone(),
            provider: provider.clone(),
            game_key,
            mod_id: mod_id.clone(),
            url,
        });
    }

    Ok(pages)
}

fn round_view(install: CollectionInstallView) -> CollectionRoundView {
    let state = classify(&install);
    CollectionRoundView {
        install,
        prompt: state.prompt,
        waiting: None,
        finished: state.finished,
    }
}

/// Applies one decision to the member it names, then drives the next round. The FOMOD wizard and the
/// "already in library" handoffs stay intact: choices go through the shared import, a picked file that turns out to be
/// a FOMOD pauses again with a fresh choice prompt instead of losing the wizard.
#[allow(
    clippy::too_many_arguments,
    reason = "a decision has the collection key, the member, the answer, progress and cancellation"
)]
pub fn resolve_prompt(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    item: &str,
    decision: CollectionDecision,
    downloads: &Downloads,
    progress: &mut dyn FnMut(CollectionProgress),
    cancel: Option<&AtomicBool>,
) -> Result<CollectionRoundView, AppError> {
    match decision {
        CollectionDecision::Choice { path, choices } => {
            download::import_file(
                root,
                game_id,
                profile,
                collection,
                item,
                Path::new(&path),
                Some(choices),
            )?;
        }
        CollectionDecision::File { path } => {
            match download::import_file(
                root,
                game_id,
                profile,
                collection,
                item,
                Path::new(&path),
                None,
            ) {
                Ok(_) => {}
                Err(error) if error.code == HAND_PICKED_NEEDS_CHOICE => {
                    return choice_prompt_for_file(
                        root, game_id, profile, collection, item, &path, error,
                    );
                }
                Err(error) => return Err(error),
            }
        }
        CollectionDecision::MarkDone => {
            session::mark_done(root, game_id, profile, collection, item)?;
        }
        CollectionDecision::Skip { permanent } => {
            session::skip(root, game_id, profile, collection, item, permanent)?;
        }
    }

    run_round(
        root, game_id, profile, collection, downloads, progress, cancel,
    )
}

/// Rebuilds the FOMOD prompt for a picked file whose archive needs choices, so the wizard can carry on after the
/// manual-file handoff just as it does after an automatic download.
fn choice_prompt_for_file(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    item: &str,
    path: &str,
    original: AppError,
) -> Result<CollectionRoundView, AppError> {
    let (module_name, steps) = match crate::import::inspect(root, game_id, Path::new(path))? {
        crate::import::model::ZipInspect::Fomod { module_name, steps } => (module_name, steps),
        crate::import::model::ZipInspect::Plain => return Err(original),
    };
    let name = session::status(root, game_id, profile, collection)?
        .plan
        .item(item)
        .map_or_else(|| item.to_string(), |found| found.name.clone());
    Ok(round_view(CollectionInstallView {
        needs_choice: vec![crate::collections::model::CollectionChoiceNeeded {
            key: item.to_string(),
            name,
            path: path.to_string(),
            module_name,
            steps,
        }],
        ..view(root, game_id, profile, collection)?
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collections::model::{CollectionChoiceNeeded, CollectionFailure};
    use std::collections::BTreeMap;

    fn round(choice: bool, manual: bool, failed: bool, pending: u32) -> CollectionInstallView {
        let manual_item = crate::domain::collection::CollectionItem {
            key: "manual".into(),
            name: "Manual mod".into(),
            version: Some("1".into()),
            author: None,
            category: None,
            summary: String::new(),
            picture_url: None,
            optional: false,
            phase: 0,
            source: crate::domain::collection::CollectionSource::Manual,
            file_name: None,
            size_kb: 0,
            md5: None,
            update_policy: crate::domain::collection::CollectionUpdatePolicy::Exact,
            choices: Vec::new(),
            patches: BTreeMap::new(),
            instructions: "Install by hand".into(),
            adult: false,
        };
        let plan = crate::domain::collection::CollectionPlan {
            format: crate::domain::collection::PLAN_FORMAT,
            info: crate::domain::collection::CollectionInfo {
                revision_label: String::new(),
                provider: "test-provider".into(),
                game_key: "example".into(),
                slug: "example".into(),
                revision: 1,
                latest_revision: 1,
                name: "Example".into(),
                author: None,
                summary: String::new(),
                description: String::new(),
                instructions: String::new(),
                picture_url: None,
                adult: false,
                total_size_kb: 0,
                mod_count: 1,
                game_versions: Vec::new(),
                changelog: None,
                url: String::new(),
            },
            items: vec![manual_item],
            rules: Vec::new(),
            issues: Vec::new(),
            order: vec!["manual".into()],
            has_bundles: false,
            has_patches: false,
            limited: false,
        };
        let counts = crate::domain::collection::CollectionCounts {
            total: 1,
            pending,
            ..crate::domain::collection::CollectionCounts::default()
        };
        let session = CollectionSessionView {
            profile: "Test Profile".into(),
            origin: crate::domain::collection::CollectionOrigin {
                provider: "test-provider".into(),
                game_key: "example".into(),
                slug: "example".into(),
                revision: 1,
                latest_revision: 1,
                revision_label: String::new(),
                name: "Example".into(),
                author: None,
                url: String::new(),
                excluded: Vec::new(),
                manual: Vec::new(),
                added: Vec::new(),
                families: Vec::new(),
                disabled: Vec::new(),
                installed_at: 0,
            },
            plan,
            statuses: BTreeMap::new(),
            counts,
            next: None,
            update: false,
        };
        CollectionInstallView {
            profile: "Test Profile".into(),
            installed: Vec::new(),
            failed: if failed {
                vec![CollectionFailure {
                    key: "failing".into(),
                    name: "Failing mod".into(),
                    error: AppError::user("test.failure"),
                }]
            } else {
                Vec::new()
            },
            needs_choice: if choice {
                vec![CollectionChoiceNeeded {
                    key: "fomod".into(),
                    name: "Fomod mod".into(),
                    path: "/tmp/fomod.zip".into(),
                    module_name: "Example module".into(),
                    steps: Vec::new(),
                }]
            } else {
                Vec::new()
            },
            needs_user: if manual {
                vec!["manual".into()]
            } else {
                Vec::new()
            },
            skipped: Vec::new(),
            warnings: Vec::new(),
            cancelled: false,
            session,
        }
    }

    #[test]
    fn a_round_with_no_pending_members_is_finished_without_a_prompt() {
        let state = classify(&round(false, false, false, 0));
        assert!(state.finished);
        assert!(state.prompt.is_none());
    }

    #[test]
    fn a_round_pauses_on_a_fomod_choice_before_a_manual_member() {
        let state = classify(&round(true, true, false, 2));
        assert!(!state.finished);

        match state.prompt {
            Some(CollectionPrompt::Choice { item, .. }) => assert_eq!(item, "fomod"),
            other => panic!("expected a FOMOD choice prompt, got {other:?}"),
        }
    }

    #[test]
    fn a_round_with_only_a_manual_member_pauses_on_that_member() {
        let state = classify(&round(false, true, false, 1));
        assert!(!state.finished);

        match state.prompt {
            Some(CollectionPrompt::Manual { item, .. }) => assert_eq!(item, "manual"),
            other => panic!("expected a manual prompt, got {other:?}"),
        }
    }

    #[test]
    fn a_round_with_failures_and_pending_members_is_not_finished() {
        let round = round_view(round(false, false, true, 1));
        assert!(!round.finished);
        assert_eq!(round.install.failed.len(), 1);
    }

    /// A session with one free-provider member. The test keyring has no saved key, so a real round leaves the member
    /// in `needs_user`; the fixtures below model that outcome and the download that follows it.
    fn provider_fixture(
        name: &str,
    ) -> (
        Data,
        String,
        String,
        crate::domain::collection::CollectionPlan,
    ) {
        use crate::collections::model::{CollectionInstallRequest, CollectionPosition};
        use crate::domain::collection::{
            CollectionInfo, CollectionItem, CollectionPlan, CollectionUpdatePolicy, PLAN_FORMAT,
        };

        let _game = crate::test_support::games::full_root();
        let root = Data::new(crate::test_support::scratch!(name));
        let plan = CollectionPlan {
            format: PLAN_FORMAT,
            info: CollectionInfo {
                revision_label: String::new(),
                provider: "nexus".into(),
                game_key: "full-root".into(),
                slug: "example".into(),
                revision: 1,
                latest_revision: 1,
                name: "Example".into(),
                author: None,
                summary: String::new(),
                description: String::new(),
                instructions: String::new(),
                picture_url: None,
                adult: false,
                total_size_kb: 0,
                mod_count: 1,
                game_versions: Vec::new(),
                changelog: None,
                url: "https://www.nexusmods.com/games/full-root/collections/example".into(),
            },
            items: vec![CollectionItem {
                key: "nexus:7".into(),
                name: "Example mod".into(),
                version: Some("1".into()),
                author: None,
                category: None,
                summary: String::new(),
                picture_url: None,
                optional: false,
                phase: 0,
                source: CollectionSource::Provider {
                    provider: "nexus".into(),
                    mod_id: "7".into(),
                    file_id: "9".into(),
                    part_id: None,
                },
                file_name: None,
                size_kb: 0,
                md5: None,
                update_policy: CollectionUpdatePolicy::Exact,
                choices: Vec::new(),
                patches: BTreeMap::new(),
                instructions: String::new(),
                adult: false,
            }],
            rules: Vec::new(),
            issues: Vec::new(),
            order: vec!["nexus:7".into()],
            has_bundles: false,
            has_patches: false,
            limited: false,
        };
        let data = root.game("full-root");
        crate::collections::cache::write_plan(&data, &plan).unwrap();
        let request = CollectionInstallRequest {
            link: plan.info.url.clone(),
            revision: Some(1),
            profile: None,
            profile_name: "Collection".into(),
            selected: vec!["nexus:7".into()],
            position: CollectionPosition::Bottom,
            activate: true,
        };
        let install = session::start_with_plan(&root, "full-root", &request, plan.clone()).unwrap();
        (
            root,
            install.profile,
            "nexus:full-root:example".into(),
            plan,
        )
    }

    #[test]
    fn an_automatic_round_requests_the_page_of_a_free_provider_member() {
        let (root, profile, collection, _plan) = provider_fixture("collection-round-page");
        let downloads = Downloads::default();
        // The browser route leaves the member in `needs_user`. Model that outcome here so the page handoff is checked
        // without a network call; `download::install_automatic` produces the same view for a free account.
        let install = CollectionInstallView {
            needs_user: vec!["nexus:7".into()],
            ..settled(session::status(&root, "full-root", &profile, &collection).unwrap())
        };
        let round = finish_round(
            &root,
            "full-root",
            &profile,
            &collection,
            &downloads,
            install,
        )
        .unwrap();

        assert!(!round.finished);
        assert!(
            round.prompt.is_none(),
            "a page wait is not a manual decision"
        );
        let page = round
            .waiting
            .expect("the round waits on the provider's page");
        assert_eq!(page.game, "full-root");
        assert_eq!(page.profile, profile);
        assert_eq!(page.collection, collection);
        assert_eq!(page.item, "nexus:7");
        assert_eq!(page.name, "Example mod");
        assert_eq!(page.provider, "nexus");
        assert_eq!(page.game_key, "full-root");
        assert_eq!(page.mod_id, "7");
        assert!(page.url.contains("full-root"));
        assert!(round.install.needs_user.is_empty());
    }

    #[test]
    fn an_owned_download_resumes_the_automatic_round_without_a_decision() {
        let (root, profile, collection, _plan) = provider_fixture("collection-round-resume");
        let dir = root.game("full-root").downloads();
        std::fs::create_dir_all(&dir).unwrap();
        let zip = crate::test_support::make_zip(
            &crate::test_support::scratch!("collection-round-resume-zip"),
            "Example Mod.zip",
            &[("bin/x64/plugins/x.asi", "x")],
        );
        let archive = dir.join("Example Mod.zip");
        std::fs::copy(&zip, &archive).unwrap();
        std::fs::write(
            dir.join("Example Mod.zip.source.json"),
            r#"{"provider":"nexus","mod_id":"7","file_id":"9"}"#,
        )
        .unwrap();

        let downloads = Downloads::default();
        let round = resolve_prompt(
            &root,
            "full-root",
            &profile,
            &collection,
            "nexus:7",
            CollectionDecision::File {
                path: archive.display().to_string(),
            },
            &downloads,
            &mut |_| {},
            None,
        )
        .unwrap();

        assert!(round.finished);
        assert!(round.prompt.is_none());
        assert!(round.waiting.is_none());
        assert_eq!(round.install.session.counts.pending, 0);
        assert_eq!(round.install.session.counts.installed, 1);
    }
}
