//! Tests for the problems a plan reports before a deployment starts.

use super::*;
use crate::domain::deploy::{Deploy, Slot};
use crate::domain::resolver::model::{ModInput, PlanMode, Problem};
use std::collections::BTreeMap;
use std::sync::Arc;

fn deploy(flat: bool) -> Deploy {
    Deploy {
        slots: vec![Slot {
            dir: "Mods".to_string(),
            prefix: String::new(),
            marker: None,
            markers: Vec::new(),
            enable_file: None,
            extensions: Vec::new(),
            root_extensions: Vec::new(),
            root_folder_extensions: Vec::new(),
            flat,
        }],
        ..Deploy::default()
    }
}

fn input(id: &str, files: &[&str]) -> ModInput<'static> {
    ModInput {
        id: id.to_string().into(),
        folder: "folder".to_string().into(),
        slot: "Mods".to_string().into(),
        display_name: id.to_string().into(),
        enabled: true,
        priority: None,
        files: Arc::from(
            files
                .iter()
                .map(|file| (*file).to_string())
                .collect::<Vec<_>>(),
        ),
        layout: None,
    }
}

#[test]
fn two_files_of_one_mod_at_the_same_destination_are_a_problem() {
    let plan = plan_with_mode(
        &deploy(true),
        &[input("local:mod", &["a/x.pak", "b/x.pak"])],
        &BTreeMap::new(),
        PlanMode::Full,
    );
    assert!(plan.problems.iter().any(|problem| matches!(
        problem,
        Problem::DuplicateDestination { path, .. } if path == "Mods/x.pak"
    )));
    assert_eq!(plan.files.len(), 1);
}

#[test]
fn a_file_and_a_folder_at_the_same_path_are_a_problem() {
    let plan = plan_with_mode(
        &deploy(false),
        &[input("local:mod", &["DLC/x", "DLC/x/y"])],
        &BTreeMap::new(),
        PlanMode::Full,
    );
    assert!(
        plan.problems
            .iter()
            .any(|problem| matches!(problem, Problem::FileFolderCollision { .. }))
    );
}
