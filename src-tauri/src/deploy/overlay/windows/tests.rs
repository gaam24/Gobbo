//! The Windows overlay, end to end: the hook DLL is loaded into `cmd.exe`, which reads, lists and writes the game
//! folder. The DLL must be built first (`cargo build -p gobbo-overlay-hook`), because the tests do not build it.

use super::Session;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::deploy::overlay::run;
use crate::deploy::{Target, apply, plan_from_disk};
use crate::domain::resolver::{DeployPlan, PlanSource, PlannedFile};
use crate::test_support::games as test_games;
use crate::test_support::{scratch, write};
use std::sync::Arc;

/// The overlay plan of the mirror fixture for these mods, read from the library on disk.
fn overlay_plan(library: &Path, folders: &[&str]) -> DeployPlan {
    let deploy = test_games::mirror().deploy;
    let profile = crate::test_support::profile_mods(folders, "");
    plan_from_disk(&deploy, library, &profile).unwrap()
}

const POWERSHELL: &str = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";

/// Runs a PowerShell script file in the session, with the arguments after its path.
fn run_script_file(session: &Session, script: &Path, args: &[String]) {
    let mut all = vec![
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        script.display().to_string(),
    ];
    all.extend_from_slice(args);
    run(session, POWERSHELL, &all).unwrap();
}

/// Asserts that the script saw the deployed archive as gone, and that the game's own copy is still there.
fn assert_game_copy_kept(out: &Path, folder: &Path) {
    assert_eq!(
        fs::read_to_string(out.join("state.txt")).unwrap().trim(),
        "False"
    );
    assert_eq!(
        fs::read_to_string(folder.join("b.archive")).unwrap(),
        "game-b"
    );
}

/// A game with two archives in a folder of its own, and a mod with an archive of its own.
fn fixture(name: &str) -> (PathBuf, PathBuf, Target) {
    let base = scratch!(name);
    let target = crate::test_support::target(&base);
    let root = target.root.clone();
    let library = base.join("library");
    write(&root.join("archive/pc/mod/a.archive"), "game-a");
    write(&root.join("archive/pc/mod/b.archive"), "game-b");
    write(&library.join("modA/archive/pc/mod/c.archive"), "mod-c");
    (library, root, target)
}

fn deploy_mod_a(library: &Path, target: &Target) -> Session {
    let deploy_plan = overlay_plan(library, &["modA"]);
    apply(&deploy_plan, library, target).unwrap();
    Session::at(&target.state, &target.root)
}

/// Runs a batch file in the session. Its lines run in order, and the exit code is the one of the last line.
fn run_batch(session: &Session, dir: &Path, lines: &[String]) -> i32 {
    let script = dir.join("probe.cmd");
    let mut body = String::from("@echo off\r\n");

    for line in lines {
        body.push_str(line);
        body.push_str("\r\n");
    }

    fs::write(&script, body).unwrap();
    run(
        session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), script.display().to_string()],
    )
    .unwrap()
}

/// A one-mod, one-file plan for the preview warnings.
fn single_file_plan(path: &str, game_path: bool) -> DeployPlan {
    DeployPlan {
        files: vec![PlannedFile {
            source: 0,
            file: 0,
            dest: path.into(),
            game_path,
        }],
        sources: vec![PlanSource {
            id: "modA".into(),
            folder: "modA".into(),
            files: Arc::from(vec![path.to_string()]),
        }],
        file_count: 1,
        ..DeployPlan::default()
    }
}

/// The snapshot and the journal of the session, joined; a whiteout may live in either until a compaction.
fn saved_whiteouts(layers: &gobbo_overlay_core::Layers) -> String {
    let mut text = fs::read_to_string(layers.whiteouts_file()).unwrap_or_default();
    text.push_str(&fs::read_to_string(layers.whiteouts_journal_file()).unwrap_or_default());
    text
}

#[test]
fn a_deployed_reserved_dll_gets_a_preview_warning() {
    let plan = single_file_plan("bin/x64/ws2_32.dll", true);
    let warnings = reserved_dll_warnings(&plan);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, "preview.reserved_dll");
    assert_eq!(
        warnings[0].params.get("path").map(String::as_str),
        Some("bin/x64/ws2_32.dll")
    );
    assert_eq!(
        warnings[0].params.get("name").map(String::as_str),
        Some("ws2_32.dll")
    );
}

#[test]
fn a_reserved_dll_in_a_mod_folder_is_not_a_game_path_warning() {
    let plan = single_file_plan("Mods/modA/ws2_32.dll", false);
    assert!(reserved_dll_warnings(&plan).is_empty());
}

mod dlls;
mod game_files;
mod imports;
mod oracle;
mod program_calls;
