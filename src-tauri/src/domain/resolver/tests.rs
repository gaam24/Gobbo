//! Planner tests: which file each mod places, who wins conflicts and the settings rows. The fixtures are here, and each
//! topic has a file of its own.

mod conflicts;
mod folders;
mod mirror;
mod problems;
mod settings;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use crate::domain::deploy::Deploy;
use crate::domain::pathkey::path_key;
use crate::test_support::games as test_games;

fn slot_game() -> Deploy {
    test_games::slot().deploy
}

fn mirror_game() -> Deploy {
    test_games::mirror().deploy
}

/// The slot game with its `Mods` slot made flat, so two mods can write the same file in it.
fn flat() -> Deploy {
    let mut deploy = slot_game();
    deploy.slots[0].flat = true;
    deploy.shared.clear();
    deploy
}

/// The slot game without the shared scripts rule.
fn plain() -> Deploy {
    let mut deploy = slot_game();
    deploy.shared.clear();
    deploy
}

fn m<'a>(id: &'a str, enabled: bool, files: &[&str]) -> ModInput<'a> {
    ModInput {
        id: id.into(),
        folder: id.into(),
        slot: "Mods".into(),
        display_name: id.into(),
        enabled,
        priority: None,
        files: files.iter().map(ToString::to_string).collect(),
        layout: None,
    }
}

/// A mod of a game without slots: every file keeps its game path.
fn mirrored<'a>(id: &'a str, files: &[&str]) -> ModInput<'a> {
    ModInput {
        slot: Cow::Borrowed(""),
        ..m(id, true, files)
    }
}

fn run(deploy: &Deploy, mods: &[ModInput<'_>]) -> DeployPlan {
    plan_with_mode(deploy, mods, &BTreeMap::new(), PlanMode::Full)
}

fn choosing(deploy: &Deploy, mods: &[ModInput<'_>], path: &str, family: &str) -> DeployPlan {
    let winners = BTreeMap::from([(path_key(path), family.to_string())]);
    plan_with_mode(deploy, mods, &winners, PlanMode::Full)
}

#[test]
fn a_layout_profile_gives_the_same_summary_as_the_file_scan() {
    let deploy = mirror_game();
    let files: Arc<[String]> = [
        "Mods/modA/content/a.ws",
        "bin/x64/version.dll",
        "r6/scripts/modA/script.reds",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>()
    .into();
    let profile = crate::domain::resolver::layout_profile(&deploy, &files);
    assert_ne!(profile.materialized, Materialized::Unknown);

    let mut with_layout = m("modA", true, &["ignored"]);
    with_layout.files = Arc::clone(&files);
    with_layout.layout = Some(&profile);
    let mut without_layout = with_layout.clone();
    without_layout.layout = None;

    let plan = |input: &ModInput<'_>| {
        plan_with_mode(
            &deploy,
            std::slice::from_ref(input),
            &BTreeMap::new(),
            PlanMode::Summary,
        )
    };
    assert_eq!(plan(&with_layout), plan(&without_layout));
}

#[test]
fn a_slot_only_list_has_nothing_to_materialize() {
    let deploy = slot_game();
    let files: Vec<String> = vec!["Mods/modA/content/a.ws".to_string()];
    let profile = crate::domain::resolver::layout_profile(&deploy, &files);
    assert_eq!(profile.materialized, Materialized::None);
    assert!(!profile.mirrored_only);
}

fn file_set(plan: &DeployPlan) -> Vec<String> {
    let mut out: Vec<String> = plan
        .files
        .iter()
        .map(|file| format!("{}/{}", plan.folder(file), plan.rel_path(file)))
        .collect();
    out.sort();
    out
}

fn dests(plan: &DeployPlan) -> Vec<String> {
    let mut out: Vec<String> = plan.files.iter().map(|f| f.dest.clone()).collect();
    out.sort();
    out
}
