use super::model::{CollectionDecision, CollectionPrompt};
use super::prompt::settled;
use super::run::{finish_round, round_view};
use super::*;
use crate::collections::model::{
    CollectionChoiceNeeded, CollectionFailure, CollectionInstallView, CollectionSessionView,
};
use crate::collections::session;
use crate::context::Data;
use crate::domain::collection::CollectionSource;
use crate::error::AppError;
use crate::sources::downloads::Downloads;
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
                choice: crate::import::model::ChoiceNeeded {
                    path: "/tmp/fomod.zip".into(),
                    module_name: "Example module".into(),
                    steps: Vec::new(),
                },
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
