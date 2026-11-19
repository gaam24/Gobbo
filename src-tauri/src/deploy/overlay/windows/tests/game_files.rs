//! The game's files through the layers: what it reads, lists, writes and deletes while it runs in the overlay.

use super::Session;
use std::fs;
use std::path::Path;

use super::*;
use crate::deploy::overlay::run;
use crate::deploy::{apply, undo};
use crate::launch::watch;
use crate::test_support::scratch;

/// The whole game folder, not only the DLLs: a program that writes under the root leaves the physical folder unchanged.
#[test]
fn a_run_through_the_overlay_leaves_the_game_folder_unchanged() {
    let (library, root, target) = fixture("clean-folder");
    let session = deploy_mod_a(&library, &target);
    let before = watch::take(&root);
    let out = scratch!("clean-folder-out");
    run_batch(
        &session,
        &out,
        &[format!(
            "echo written > \"{}\"",
            root.join(r"archive\pc\mod\new.archive").display()
        )],
    );
    let after = watch::take(&root);
    let changes = watch::changes(&before, &after, &[]);
    assert!(changes.is_empty(), "{changes:?}");
    assert!(
        session
            .overwrite
            .join(r"archive\pc\mod\new.archive")
            .is_file()
    );
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_hooked_program_reads_lists_and_writes_through_the_layers() {
    let (library, root, target) = fixture("hooked");
    let deploy_plan = overlay_plan(&library, &["modA"]);
    apply(&deploy_plan, &library, &target).unwrap();
    let session = Session::at(&target.state, &root);

    let folder = root.join("archive/pc/mod");
    let out = scratch!("hooked-out");
    let script = out.join("probe.cmd");
    fs::write(
        &script,
        format!(
            "@echo off\r\n\
             type \"{c}\" > \"{out}\\c.txt\"\r\n\
             type \"{b}\" > \"{out}\\b.txt\"\r\n\
             dir /b \"{folder}\" > \"{out}\\list.txt\"\r\n\
             echo written > \"{new}\"\r\n\
             echo changed > \"{b}\"\r\n",
            c = folder.join("c.archive").display(),
            b = folder.join("b.archive").display(),
            folder = folder.display(),
            new = folder.join("new.archive").display(),
            out = out.display(),
        ),
    )
    .unwrap();

    let code = run(
        &session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), script.display().to_string()],
    )
    .unwrap();
    assert_eq!(code, 0);

    // Reads see the mod's file in the mod layer, and the game's own file in the game folder.
    assert_eq!(
        fs::read_to_string(out.join("c.txt")).unwrap().trim(),
        "mod-c"
    );
    assert_eq!(
        fs::read_to_string(out.join("b.txt")).unwrap().trim(),
        "game-b"
    );
    // The listing merges the layers: the game's files and the mod's file, each once.
    let list = fs::read_to_string(out.join("list.txt"))
        .unwrap()
        .to_lowercase();
    for name in ["a.archive", "b.archive", "c.archive"] {
        assert_eq!(list.matches(name).count(), 1, "{name} in {list}");
    }
    // Writes land in the upper layer, also when they change a file of the game (copy-up). The game folder and
    // the mod layer keep their files.
    let upper = session.overwrite.join("archive/pc/mod");
    assert_eq!(
        fs::read_to_string(upper.join("new.archive"))
            .unwrap()
            .trim(),
        "written"
    );
    assert_eq!(
        fs::read_to_string(upper.join("b.archive")).unwrap().trim(),
        "changed"
    );
    assert!(!folder.join("new.archive").exists());
    assert_eq!(
        fs::read_to_string(folder.join("b.archive")).unwrap(),
        "game-b"
    );
    assert_eq!(
        fs::read_to_string(session.mods.join("archive/pc/mod/c.archive")).unwrap(),
        "mod-c"
    );

    undo(&target.journal_roots()).unwrap();
}

#[test]
fn the_hook_records_decisions_that_the_app_can_read() {
    let (library, root, target) = fixture("decisions");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("decisions-out");
    run_batch(
        &session,
        &out,
        &[format!(
            "echo changed >> \"{}\"",
            folder.join("b.archive").display()
        )],
    );

    let (decisions, total) = crate::deploy::overlay::decisions(&target.state, 200);
    assert!(total >= 2, "total {total}");
    assert!(
        decisions.iter().any(
            |decision| decision.kind == gobbo_overlay_decisions::Kind::Write
                && decision.path.contains("b.archive")
        ),
        "{decisions:?}"
    );
    assert!(
        decisions.iter().any(
            |decision| decision.kind == gobbo_overlay_decisions::Kind::CopyUp
                && decision.path.contains("b.archive")
        ),
        "{decisions:?}"
    );
    undo(&target.journal_roots()).unwrap();
}

/// A write into a folder that no layer shows must fail like Windows, without creating the folder in the upper layer.
#[test]
fn a_write_into_a_missing_folder_fails_and_creates_nothing() {
    let (library, root, target) = fixture("missing-parent");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    let missing = folder.join("missing").join("new.txt");
    let out = scratch!("missing-parent-out");

    run_batch(
        &session,
        &out,
        &[format!("echo x > \"{}\" 2>nul", missing.display())],
    );

    // Windows refuses a write into a folder that does not exist; the overlay must not create the folder in upper.
    assert!(!missing.exists());
    assert!(!folder.join("missing").exists());
    assert!(!session.overwrite.join("archive/pc/mod/missing").exists());
    undo(&target.journal_roots()).unwrap();
}

/// Deploys mod A of the fixture and returns the session.
#[test]
fn a_game_deleting_its_own_file_leaves_the_game_folder_alone() {
    let (library, root, target) = fixture("delete");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    let out = scratch!("delete-out");

    run_batch(
        &session,
        &out,
        &[
            format!("del \"{}\"", folder.join("b.archive").display()),
            format!(
                "if exist \"{b}\" (echo present > \"{out}\\state.txt\") else (echo gone > \"{out}\\state.txt\")",
                b = folder.join("b.archive").display(),
                out = out.display(),
            ),
        ],
    );

    assert_eq!(
        fs::read_to_string(out.join("state.txt")).unwrap().trim(),
        "gone"
    );
    // The game's own copy is never deleted: the deletion is a whiteout in the upper layer.
    assert_eq!(
        fs::read_to_string(folder.join("b.archive")).unwrap(),
        "game-b"
    );
    undo(&target.journal_roots()).unwrap();
}

/// A rename that only changes the case must not look like a taken name, and must not leave the old spelling visible.
#[test]
fn a_case_only_rename_moves_the_upper_copy_and_hides_the_game_name() {
    let (library, root, target) = fixture("case-rename");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("case-rename-out");

    run_batch(
        &session,
        &out,
        &[format!(
            "ren \"{}\" \"B.ARCHIVE\"",
            folder.join("b.archive").display()
        )],
    );

    // The upper layer has the copy under its new spelling, and not under the old one. The names are compared exactly,
    // because a Windows lookup would find `B.ARCHIVE` under either spelling.
    let upper = session.overwrite.join("archive/pc/mod");
    assert_eq!(
        fs::read_to_string(upper.join("B.ARCHIVE")).unwrap(),
        "game-b"
    );
    let names: Vec<String> = fs::read_dir(&upper)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().any(|name| name == "B.ARCHIVE"), "{names:?}");
    assert!(!names.iter().any(|name| name == "b.archive"), "{names:?}");
    // The game folder keeps its own file, and the old spelling is hidden so that it does not come back.
    assert_eq!(
        fs::read_to_string(folder.join("b.archive")).unwrap(),
        "game-b"
    );
    assert!(
        saved_whiteouts(&session.layers())
            .to_lowercase()
            .contains("b.archive")
    );
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_deletion_is_still_in_effect_in_the_next_run_of_the_game() {
    let (library, root, target) = fixture("delete-next-run");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    let first = scratch!("delete-next-run-first");
    let second = scratch!("delete-next-run-second");
    let check = |out: &Path| {
        format!(
            "if exist \"{b}\" (echo present > \"{out}\\state.txt\") else (echo gone > \"{out}\\state.txt\")",
            b = folder.join("b.archive").display(),
            out = out.display(),
        )
    };

    run_batch(
        &session,
        &first,
        &[format!("del \"{}\"", folder.join("b.archive").display())],
    );
    run_batch(&session, &second, &[check(&second)]);

    assert_eq!(
        fs::read_to_string(second.join("state.txt")).unwrap().trim(),
        "gone"
    );
    assert!(
        saved_whiteouts(&session.layers())
            .to_lowercase()
            .contains("b.archive")
    );
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_rebuilt_layer_starts_without_the_whiteouts_of_the_old_one() {
    let (library, root, target) = fixture("new-layer");
    let deploy_plan = overlay_plan(&library, &["modA"]);
    apply(&deploy_plan, &library, &target).unwrap();
    let layers = Session::at(&target.state, &root).layers();
    let out = scratch!("new-layer-out");
    run_batch(
        &Session::at(&target.state, &root),
        &out,
        &[format!(
            "del \"{}\"",
            root.join(r"archive\pc\mod\b.archive").display()
        )],
    );
    assert!(
        saved_whiteouts(&layers)
            .to_lowercase()
            .contains("b.archive")
    );
    undo(&target.journal_roots()).unwrap();

    // The same mods reuse the layer, and its whiteouts stay with it.
    apply(&deploy_plan, &library, &target).unwrap();
    assert!(
        saved_whiteouts(&layers)
            .to_lowercase()
            .contains("b.archive")
    );
    undo(&target.journal_roots()).unwrap();

    // Other mods build the layer again, so the old whiteouts go.
    let other = overlay_plan(&library, &[]);
    apply(&other, &library, &target).unwrap();
    assert!(!layers.whiteouts_file().exists());
    assert!(!layers.whiteouts_journal_file().exists());
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_file_deleted_by_the_game_can_be_written_again() {
    let (library, root, target) = fixture("rewrite");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    let out = scratch!("rewrite-out");

    run_batch(
        &session,
        &out,
        &[
            format!("del \"{}\"", folder.join("b.archive").display()),
            format!("echo again > \"{}\"", folder.join("b.archive").display()),
            format!(
                "type \"{}\" > \"{}\\read.txt\"",
                folder.join("b.archive").display(),
                out.display()
            ),
        ],
    );

    assert_eq!(
        fs::read_to_string(out.join("read.txt")).unwrap().trim(),
        "again"
    );
    assert_eq!(
        fs::read_to_string(folder.join("b.archive")).unwrap(),
        "game-b"
    );
    assert_eq!(
        fs::read_to_string(session.overwrite.join("archive/pc/mod/b.archive"))
            .unwrap()
            .trim(),
        "again"
    );
    undo(&target.journal_roots()).unwrap();
}

/// The 32-bit hook must see the same layers as the 64-bit one. `SysWOW64` runs the 32-bit `cmd`, so this exercises the
/// 32-bit DLL, its pointer-sized NT layouts and its handle values.
#[test]
fn a_32_bit_program_reads_and_writes_through_the_layers() {
    const CMD32: &str = r"C:\Windows\SysWOW64\cmd.exe";
    if !Path::new(CMD32).is_file() {
        // A 32-bit test needs a 64-bit Windows installation with WOW64.
        return;
    }
    let (library, root, target) = fixture("wow64");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    let out = scratch!("wow64-out");
    let script = out.join("probe.cmd");
    fs::write(
        &script,
        format!(
            "@echo off\r\n\
             type \"{c}\" > \"{out}\\c.txt\"\r\n\
             echo changed > \"{b}\"\r\n",
            c = folder.join("c.archive").display(),
            b = folder.join("b.archive").display(),
            out = out.display(),
        ),
    )
    .unwrap();

    let code = run(
        &session,
        CMD32,
        &["/c".to_string(), script.display().to_string()],
    )
    .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        fs::read_to_string(out.join("c.txt")).unwrap().trim(),
        "mod-c"
    );
    assert_eq!(
        fs::read_to_string(session.overwrite.join("archive/pc/mod/b.archive"))
            .unwrap()
            .trim(),
        "changed"
    );
    assert_eq!(
        fs::read_to_string(folder.join("b.archive")).unwrap(),
        "game-b"
    );
    undo(&target.journal_roots()).unwrap();
}

/// A 64-bit game may start a 32-bit helper. The 64-bit hook cannot inject the 32-bit DLL itself, so it starts the
/// 32-bit injector; this test proves the child is hooked before it touches the game folder.
#[test]
fn a_32_bit_child_of_a_64_bit_program_is_hooked_too() {
    const CMD32: &str = r"C:\Windows\SysWOW64\cmd.exe";
    if !Path::new(CMD32).is_file() {
        return;
    }
    let (library, root, target) = fixture("wow64-child");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    let out = scratch!("wow64-child-out");
    let child = out.join("child.cmd");
    fs::write(
        &child,
        format!(
            "@echo off\r\n\
             type \"{c}\" > \"{out}\\child-c.txt\"\r\n\
             echo child > \"{b}\"\r\n",
            c = folder.join("c.archive").display(),
            b = folder.join("b.archive").display(),
            out = out.display(),
        ),
    )
    .unwrap();
    let parent = out.join("parent.cmd");
    fs::write(
        &parent,
        format!("@echo off\r\n\"{CMD32}\" /c \"{}\"\r\n", child.display()),
    )
    .unwrap();

    let code = run(
        &session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), parent.display().to_string()],
    )
    .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        fs::read_to_string(out.join("child-c.txt")).unwrap().trim(),
        "mod-c"
    );
    assert_eq!(
        fs::read_to_string(session.overwrite.join("archive/pc/mod/b.archive"))
            .unwrap()
            .trim(),
        "child"
    );
    assert_eq!(
        fs::read_to_string(folder.join("b.archive")).unwrap(),
        "game-b"
    );
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_save_written_under_a_temporary_name_and_renamed_stays_in_the_upper_layer() {
    let (library, root, target) = fixture("rename");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    let out = scratch!("rename-out");

    run_batch(
        &session,
        &out,
        &[
            format!("echo saved > \"{}\"", folder.join("save.tmp").display()),
            format!(
                "move /y \"{}\" \"{}\"",
                folder.join("save.tmp").display(),
                folder.join("save.sav").display()
            ),
            format!(
                "type \"{}\" > \"{}\\read.txt\"",
                folder.join("save.sav").display(),
                out.display()
            ),
        ],
    );

    assert_eq!(
        fs::read_to_string(out.join("read.txt")).unwrap().trim(),
        "saved"
    );
    assert!(!folder.join("save.tmp").exists());
    assert!(!folder.join("save.sav").exists());
    assert!(session.overwrite.join("archive/pc/mod/save.sav").is_file());
    undo(&target.journal_roots()).unwrap();
}

/// A program that spells the game folder with another letter case outside ASCII writes through the layer, not into the
/// game folder. NTFS finds the folder under either spelling, so the hook must find it too. `powershell` is the program to
/// run: the 64-bit one, or the 32-bit one from `SysWOW64`, which checks the 32-bit hook the same way.
fn writes_through_the_layer_when_spelled_in_another_case(name: &str, powershell: &str) {
    let base = scratch!(name);
    let root = base.join("Gra").join("ś");
    let library = base.join("library");
    write(&root.join("archive/pc/mod/a.archive"), "game-a");
    write(&library.join("modA/archive/pc/mod/c.archive"), "mod-c");
    let mut target = crate::test_support::target(&base);
    target.root = root.clone();
    target.game_root = root.clone();
    let session = deploy_mod_a(&library, &target);

    let before = watch::take(&root);
    // The same folder, with its last letter in upper case. PowerShell reads its script as UTF-8 when the file has a byte
    // order mark; cmd would garble the letter in its batch file and on its command line (the console code page).
    let spelled = root.display().to_string().replace('ś', "Ś");
    let out = scratch!(&format!("{name}-out"));
    let script = out.join("write.ps1");
    fs::write(
        &script,
        format!(
            "\u{feff}[System.IO.File]::WriteAllText('{spelled}\\archive\\pc\\mod\\new.archive', 'written')\r\n"
        ),
    )
    .unwrap();
    run(
        &session,
        powershell,
        &[
            "-NoProfile".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
            "-File".to_string(),
            script.display().to_string(),
        ],
    )
    .unwrap();
    let after = watch::take(&root);

    let changes = watch::changes(&before, &after, &[]);
    assert!(changes.is_empty(), "{changes:?}");
    assert!(
        session
            .overwrite
            .join(r"archive\pc\mod\new.archive")
            .is_file()
    );
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_64_bit_program_spelling_the_game_folder_in_another_case_writes_through_the_layer() {
    writes_through_the_layer_when_spelled_in_another_case("non-ascii-case-64", POWERSHELL);
}

#[test]
fn a_32_bit_program_spelling_the_game_folder_in_another_case_writes_through_the_layer() {
    writes_through_the_layer_when_spelled_in_another_case(
        "non-ascii-case-32",
        r"C:\Windows\SysWOW64\WindowsPowerShell\v1.0\powershell.exe",
    );
}

/// A copy-up temporary file is not a file of the game: the program does not see it in a listing. It is written here
/// through the hook, as an interrupted copy would leave it.
#[test]
fn a_copy_temporary_file_is_hidden_from_the_game_listing() {
    let (library, root, target) = fixture("copy-temp-hidden");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    let out = scratch!("copy-temp-hidden-out");
    run_batch(
        &session,
        &out,
        &[
            format!(
                "echo left > \"{}\"",
                folder.join(".gobbo-copy-4294967290-0.tmp").display()
            ),
            format!(
                "dir /b /a \"{}\" > \"{}\"",
                folder.display(),
                out.join("list.txt").display()
            ),
        ],
    );
    let list = fs::read_to_string(out.join("list.txt")).unwrap();
    assert!(!list.contains("gobbo-copy"), "{list}");
    assert!(list.contains("a.archive"), "{list}");
    undo(&target.journal_roots()).unwrap();
}

/// A copy-up that an earlier run interrupted leaves its temporary file in the profile's layer; the next launch removes it.
#[test]
fn an_interrupted_copy_temporary_file_is_removed_at_the_next_launch() {
    let (library, _root, target) = fixture("copy-temp-swept");
    let session = deploy_mod_a(&library, &target);
    let stale = session
        .overwrite
        .join("archive/pc/mod/.gobbo-copy-4294967290-3.tmp");
    write(&stale, "partial");
    let out = scratch!("copy-temp-swept-out");
    run_batch(
        &session,
        &out,
        &[format!("echo ok > \"{}\"", out.join("ok.txt").display())],
    );
    assert!(!stale.exists());
    assert!(out.join("ok.txt").is_file());
    undo(&target.journal_roots()).unwrap();
}

/// A folder's listing merges the three layers, with the game folder read one entry at a time: a name that a mod shows
/// is listed once, a deleted game file is not listed, and every other game file is.
#[test]
fn a_listing_merges_the_game_folder_read_entry_by_entry() {
    let base = scratch!("listing-stream");
    let target = crate::test_support::target(&base);
    let root = target.root.clone();
    let library = base.join("library");
    let folder = root.join("archive/pc/mod");
    for number in 0..50 {
        write(&folder.join(format!("g{number:02}.archive")), "game");
    }
    write(&library.join("modA/archive/pc/mod/g03.archive"), "mod");
    write(&library.join("modA/archive/pc/mod/new.archive"), "mod");
    let session = deploy_mod_a(&library, &target);
    let out = scratch!("listing-stream-out");
    run_batch(
        &session,
        &out,
        &[
            format!("del \"{}\"", folder.join("g07.archive").display()),
            format!(
                "dir /b /a-d \"{}\" > \"{}\"",
                folder.display(),
                out.join("list.txt").display()
            ),
        ],
    );
    let list = fs::read_to_string(out.join("list.txt")).unwrap();
    let names: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let unique: std::collections::BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(
        unique.len(),
        names.len(),
        "a name is listed twice: {names:?}"
    );
    assert_eq!(names.len(), 50, "{names:?}");
    assert!(unique.contains("new.archive"));
    assert!(unique.contains("g03.archive"));
    assert!(!unique.contains("g07.archive"));
    assert!(unique.contains("g49.archive"));
    undo(&target.journal_roots()).unwrap();
}

/// The answers a program gets in one scripted session, with the times, sizes and volume facts of a machine removed. The
/// session runs the commands a program uses for listings, attributes, deletions, removals, renames and errors. Each
/// command reports its outcome as `ok` or `failed`, and its own output (an error text, for one) goes to the same file.
fn scripted_answers(name: &str) -> Vec<String> {
    let (library, root, target) = fixture(name);
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    fs::create_dir_all(folder.join("empty")).unwrap();
    let out = scratch!(&format!("{name}-out"));
    let answers = out.join("answers.txt");
    let commands = [
        ("dir-all", format!("dir /a \"{}\"", folder.display())),
        ("dir-names", format!("dir /b \"{}\"", folder.display())),
        (
            "exists",
            format!(
                "if exist \"{}\" (echo yes) else (echo no)",
                folder.join("a.archive").display()
            ),
        ),
        (
            "attrib",
            format!("attrib \"{}\"", folder.join("a.archive").display()),
        ),
        (
            "delete",
            format!("del \"{}\"", folder.join("b.archive").display()),
        ),
        (
            "delete-missing",
            format!("del \"{}\"", folder.join("nope.archive").display()),
        ),
        (
            "rmdir",
            format!("rmdir \"{}\"", folder.join("empty").display()),
        ),
        (
            "mkdir-rmdir",
            format!(
                "mkdir \"{}\" && rmdir \"{}\"",
                folder.join("made").display(),
                folder.join("made").display()
            ),
        ),
        (
            "move",
            format!(
                "move \"{}\" \"{}\"",
                folder.join("a.archive").display(),
                folder.join("moved.archive").display()
            ),
        ),
        (
            "dir-names-after",
            format!("dir /b \"{}\"", folder.display()),
        ),
    ];
    let mut lines = vec![String::from("@echo off\r\n")];
    for (label, command) in &commands {
        // The command's output goes to the answers, then its outcome: `ok` or `failed`, from its error level.
        let file = answers.display();
        lines.push(format!("({command}) >> \"{file}\" 2>&1\r\n"));
        lines.push(format!(
            "if errorlevel 1 (echo {label} failed >> \"{file}\") else (echo {label} ok >> \"{file}\")\r\n"
        ));
    }
    let script = out.join("probe.cmd");
    fs::write(&script, lines.concat()).unwrap();
    let code = run(
        &session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), script.display().to_string()],
    )
    .unwrap();
    assert_eq!(code, 0);
    let base = target.root.parent().unwrap().display().to_string();
    // cmd writes the console code page, so the file is read as bytes.
    String::from_utf8_lossy(&fs::read(&answers).unwrap())
        .lines()
        .filter(|line| {
            !line.contains("bytes free")
                && !line.contains("Directory of")
                && !line.contains("Volume")
                && !line.to_lowercase().contains("file(s)")
        })
        .map(|line| line.replace(&base, "<base>"))
        .map(|line| {
            line.split_whitespace()
                .filter(|token| {
                    // The dates and sizes of a line go; the dot entries and the names stay.
                    matches!(*token, "." | "..")
                        || !(token
                            .chars()
                            .all(|c| c.is_ascii_digit() || ".,/:".contains(c))
                            || matches!(*token, "AM" | "PM"))
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|line| !line.is_empty())
        .collect()
}

/// The answers of the native layer (the NT calls the hook redirects) and of the hooks that stay, in the order a program
/// sees them, for a scripted session. The Win32 attribute hooks are not needed for them: a program that lists, asks for
/// attributes, deletes, removes and renames gets the answers of a plain folder. The listing order is the upper layer
/// first, then the game folder; a plain NTFS folder lists its names in sorted order (a known difference).
#[test]
fn the_answers_of_a_scripted_session_are_the_expected_ones() {
    let answers = scripted_answers("answers");
    assert_eq!(
        answers,
        [
            "<DIR> .",
            "<DIR> ..",
            "c.archive",
            "a.archive",
            "b.archive",
            "<DIR> empty",
            "dir-all ok",
            "c.archive",
            "a.archive",
            "b.archive",
            "empty",
            "dir-names ok",
            "yes",
            "exists ok",
            r"A <base>\game\archive\pc\mod\a.archive",
            "attrib ok",
            "delete ok",
            r"Could Not Find <base>\game\archive\pc\mod\nope.archive",
            "delete-missing ok",
            "rmdir ok",
            "mkdir-rmdir ok",
            "move ok",
            "moved.archive",
            "c.archive",
            "dir-names-after ok",
        ]
    );
}

/// A program that writes a file or a folder under another spelling of a name the game has sees the game's spelling in the
/// listing, as Windows keeps the name of an existing entry. A name the game does not have keeps the program's spelling.
#[test]
fn a_listing_keeps_the_game_spelling_of_a_name_written_in_another_case() {
    let (library, root, target) = fixture("spelling");
    let deploy_plan = overlay_plan(&library, &["modA"]);
    apply(&deploy_plan, &library, &target).unwrap();
    let session = Session::at(&target.state, &root);

    let out = scratch!("spelling-out");
    let script = out.join("spelling.cmd");
    fs::write(
        &script,
        format!(
            "@echo off\r\n\
             echo written > \"{written}\"\r\n\
             echo new > \"{fresh}\"\r\n\
             dir /b \"{top}\" > \"{out}\\top.txt\"\r\n\
             dir /b \"{folder}\" > \"{out}\\list.txt\"\r\n",
            written = root.join(r"ARCHIVE\PC\MOD\A.ARCHIVE").display(),
            fresh = root.join(r"ARCHIVE\PC\MOD\NEW.ARCHIVE").display(),
            top = root.display(),
            folder = root.join(r"archive\pc\mod").display(),
            out = out.display(),
        ),
    )
    .unwrap();
    let code = run(
        &session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), script.display().to_string()],
    )
    .unwrap();
    assert_eq!(code, 0);

    let top: Vec<String> = fs::read_to_string(out.join("top.txt"))
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .collect();
    assert!(top.contains(&"archive".to_string()), "{top:?}");
    assert!(!top.contains(&"ARCHIVE".to_string()), "{top:?}");

    let list: Vec<String> = fs::read_to_string(out.join("list.txt"))
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .collect();
    // The game has `a.archive`; the write named it `A.ARCHIVE`, and the listing keeps the game's spelling.
    assert!(list.contains(&"a.archive".to_string()), "{list:?}");
    assert!(!list.contains(&"A.ARCHIVE".to_string()), "{list:?}");
    // A new name keeps the program's spelling.
    assert!(list.contains(&"NEW.ARCHIVE".to_string()), "{list:?}");
    undo(&target.journal_roots()).unwrap();
}

/// A name the program deleted from the game is gone: a file moved onto it in another case keeps the program's spelling,
/// as it does in a plain folder where nothing has the name any more.
#[test]
fn a_move_onto_a_deleted_name_keeps_the_programs_spelling() {
    let (library, root, target) = fixture("spelling-deleted");
    let deploy_plan = overlay_plan(&library, &["modA"]);
    apply(&deploy_plan, &library, &target).unwrap();
    let session = Session::at(&target.state, &root);

    let out = scratch!("spelling-deleted-out");
    let script = out.join("moved.cmd");
    fs::write(
        &script,
        format!(
            "@echo off\r\n\
             del \"{old}\"\r\n\
             echo new > \"{fresh}\"\r\n\
             move /y \"{fresh}\" \"{target}\"\r\n\
             dir /b \"{folder}\" > \"{out}\\list.txt\"\r\n",
            old = root.join(r"archive\pc\mod\a.archive").display(),
            fresh = root.join(r"archive\pc\mod\new.archive").display(),
            target = root.join(r"ARCHIVE\PC\MOD\A.ARCHIVE").display(),
            folder = root.join(r"archive\pc\mod").display(),
            out = out.display(),
        ),
    )
    .unwrap();
    let code = run(
        &session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), script.display().to_string()],
    )
    .unwrap();
    assert_eq!(code, 0);

    let list: Vec<String> = fs::read_to_string(out.join("list.txt"))
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .collect();
    assert!(list.contains(&"A.ARCHIVE".to_string()), "{list:?}");
    assert!(!list.contains(&"a.archive".to_string()), "{list:?}");
    undo(&target.journal_roots()).unwrap();
}

/// A move that replaces a game file under another spelling leaves one entry, under the program's spelling: the game's file
/// is gone once it is replaced, so the listing must not show it with its old spelling.
#[test]
fn a_move_that_replaces_a_game_file_shows_the_programs_spelling() {
    let (library, root, target) = fixture("replaced");
    let deploy_plan = overlay_plan(&library, &["modA"]);
    apply(&deploy_plan, &library, &target).unwrap();
    let session = Session::at(&target.state, &root);

    let out = scratch!("replaced-out");
    let script = out.join("replaced.cmd");
    let folder = root.join(r"archive\pc\mod");
    fs::write(
        &script,
        format!(
            "@echo off\r\n\
             echo new > \"{fresh}\"\r\n\
             move /y \"{fresh}\" \"{target}\"\r\n\
             dir /b \"{folder}\" > \"{out}\\list.txt\"\r\n",
            fresh = folder.join("new.archive").display(),
            target = folder.join("B.ARCHIVE").display(),
            folder = folder.display(),
            out = out.display(),
        ),
    )
    .unwrap();
    let code = run(
        &session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), script.display().to_string()],
    )
    .unwrap();
    assert_eq!(code, 0);
    let list: Vec<String> = fs::read_to_string(out.join("list.txt"))
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .collect();
    assert!(list.contains(&"B.ARCHIVE".to_string()), "{list:?}");
    assert!(!list.contains(&"b.archive".to_string()), "{list:?}");
    undo(&target.journal_roots()).unwrap();
}
