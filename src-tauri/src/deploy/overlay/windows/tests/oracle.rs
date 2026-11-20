//! The parity oracle. A random session of file calls runs once through the overlay and once in a plain copy of the merged
//! tree (what a copy deployment shows the program). The answers of both must be the same, the game folder must not
//! change, and the session must end with the same tree. A seed names a session, so a difference can be run again.

use super::*;
use std::fs;
use std::path::Path;

use crate::deploy::overlay::run;
use crate::deploy::{apply, undo};
use crate::launch::watch;
use crate::test_support::{scratch, write};

const CMD: &str = r"C:\Windows\System32\cmd.exe";

/// The paths a session works on, relative to the game folder. `e` is a file in every session; the folders are created as a
/// session needs them.
const PATHS: [&str; 10] = [
    "a.txt",
    "b.txt",
    "A.TXT",
    "d\\x.txt",
    "d\\y.txt",
    "d\\e\\z.txt",
    "e",
    "f\\g.txt",
    "d\\new.txt",
    // The folder spelled in capitals: a session may write through another spelling of `d`.
    "D\\X.TXT",
];

/// The folders a session creates and removes. None of them is a file of `PATHS`, and a folder is never moved: a folder of
/// a lower layer cannot be renamed through the overlay (ARCHITECTURE.md, the limits), which is a known difference.
const FOLDERS: [&str; 5] = ["d", "d\\e", "f", "h", "D"];

/// A small deterministic generator (xorshift), so a seed names one session.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// One file call of the session, on paths of `PATHS` (by index).
#[derive(Debug, Clone, Copy)]
enum Op {
    Write(usize, u32),
    Read(usize),
    Delete(usize),
    Move(usize, usize),
    MakeFolder(usize),
    RemoveFolder(usize),
    List(usize),
    Exists(usize),
}

fn generate(rng: &mut Rng, count: usize) -> Vec<Op> {
    (0..count)
        .map(|step| {
            let path = rng.below(PATHS.len());

            match rng.below(8) {
                0 => Op::Write(path, step as u32),
                1 => Op::Read(path),
                2 => Op::Delete(path),
                3 => Op::Move(path, rng.below(PATHS.len())),
                4 => Op::MakeFolder(rng.below(FOLDERS.len())),
                5 => Op::RemoveFolder(rng.below(FOLDERS.len())),
                6 => Op::List(rng.below(FOLDERS.len())),
                _ => Op::Exists(path),
            }
        })
        .collect()
}

/// The command line of one operation, with the game folder at `root`. The answer of every line is written to `answers`.
fn command(op: Op, root: &str) -> String {
    let at = |index: usize| format!("{root}\\{}", PATHS[index]);

    match op {
        Op::Write(path, value) => format!("echo v{value} > \"{}\"", at(path)),
        Op::Read(path) => format!("type \"{}\"", at(path)),
        Op::Delete(path) => format!("del \"{}\"", at(path)),
        Op::Move(from, to) => format!("move /y \"{}\" \"{}\"", at(from), at(to)),
        Op::MakeFolder(folder) => format!("mkdir \"{root}\\{}\"", FOLDERS[folder]),
        Op::RemoveFolder(folder) => format!("rmdir \"{root}\\{}\"", FOLDERS[folder]),
        // Sorted, so the order of a listing does not decide the answer: the layers list in their own order.
        Op::List(folder) => format!("dir /b \"{root}\\{}\" | sort", FOLDERS[folder]),
        Op::Exists(path) => format!("if exist \"{}\" (echo yes) else (echo no)", at(path)),
    }
}

/// The script of a session: every operation, its output, and whether it succeeded, then the whole tree, sorted.
fn script(ops: &[Op], root: &str, answers: &Path) -> String {
    let file = answers.display();
    let mut lines = vec![String::from("@echo off\r\n")];

    for (index, op) in ops.iter().enumerate() {
        lines.push(format!("({}) >> \"{file}\" 2>&1\r\n", command(*op, root)));
        lines.push(format!(
            "if errorlevel 1 (echo {index} failed >> \"{file}\") else (echo {index} ok >> \"{file}\")\r\n"
        ));
    }

    lines.push(format!(
        "(dir /s /b \"{root}\" | sort) >> \"{file}\" 2>&1\r\n"
    ));
    lines.concat()
}

/// The answers of a session, without what differs between two machines and two folders: the volume facts, the free
/// space, the dates and sizes of a listing, and the folder itself (replaced by `<root>`).
fn answers(path: &Path, root: &str) -> Vec<String> {
    String::from_utf8_lossy(&fs::read(path).unwrap())
        .lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            !lower.contains("bytes free")
                && !lower.contains("directory of")
                && !lower.contains("volume")
                && !lower.contains("file(s)")
        })
        .map(|line| line.replace(root, "<root>"))
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// The files of a session's start: the game folder and the mod's files, as (relative path, content). A file of the mod
/// replaces the game's file of the same name, as the overlay shows it.
struct Start {
    game: Vec<(usize, String)>,
    mod_files: Vec<(usize, String)>,
}

fn start(rng: &mut Rng) -> Start {
    let mut game = Vec::new();
    let mut mod_files = Vec::new();

    for index in 0..PATHS.len() {
        if index == 6 {
            // `e` is a file, never a folder.
            if rng.below(2) == 0 {
                game.push((index, format!("game-{index}")));
            }

            continue;
        }

        if rng.below(2) == 0 {
            game.push((index, format!("game-{index}")));
        }

        if rng.below(3) == 0 {
            mod_files.push((index, format!("mod-{index}")));
        }
    }

    Start { game, mod_files }
}

/// Writes the files of a start into `folder`, creating the folders they need.
fn write_all(folder: &Path, files: &[(usize, String)]) {
    for (index, content) in files {
        write(&folder.join(PATHS[*index]), content);
    }
}

/// Runs one seeded session and returns its answers from the overlay and from the plain copy.
fn sessions(seed: u64) -> (Vec<String>, Vec<String>) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let start = start(&mut rng);
    let ops = generate(&mut rng, 60);

    // The overlay: the game folder and the mod's library folder, deployed as the app deploys them.
    let base = scratch!(&format!("oracle-{seed}"));
    let target = crate::test_support::target(&base);
    let root = target.root.clone();
    let library = base.join("library");
    write_all(&root, &start.game);
    fs::create_dir_all(library.join("modA")).unwrap();
    write_all(&library.join("modA"), &start.mod_files);
    let deploy_plan = overlay_plan(&library, &["modA"]);
    apply(&deploy_plan, &library, &target).unwrap();
    let session = Session::at(&target.state, &target.root);
    let before = watch::take(&root);

    let out = scratch!(&format!("oracle-{seed}-out"));
    let overlay_answers = out.join("overlay.txt");
    let overlay_script = out.join("overlay.cmd");
    fs::write(
        &overlay_script,
        script(&ops, &root.display().to_string(), &overlay_answers),
    )
    .unwrap();
    run(
        &session,
        CMD,
        &["/c".to_string(), overlay_script.display().to_string()],
    )
    .unwrap();
    let after = watch::take(&root);
    let changes = watch::changes(&before, &after);
    assert!(
        changes.is_empty(),
        "seed {seed}: the game folder changed: {changes:?}"
    );

    // The plain copy of the merged tree, in a folder of its own, run without the overlay.
    // The plain copy: the game's files, then the mod's over them, as a copy deployment does. NTFS keeps the name of an
    // existing entry when a file is written over it, so the copy shows the game's spelling, and the overlay must too.
    let plain_root = base.join("plain");
    write_all(&plain_root, &start.game);
    write_all(&plain_root, &start.mod_files);
    let plain_answers = out.join("plain.txt");
    let plain_script = out.join("plain.cmd");
    fs::write(
        &plain_script,
        script(&ops, &plain_root.display().to_string(), &plain_answers),
    )
    .unwrap();
    let status = std::process::Command::new(CMD)
        .args(["/c", &plain_script.display().to_string()])
        .status()
        .unwrap();
    assert!(status.success(), "seed {seed}: the plain run failed");

    let overlay = answers(&overlay_answers, &root.display().to_string());
    let plain = answers(&plain_answers, &plain_root.display().to_string());
    undo(&target.journal_roots()).unwrap();
    (overlay, plain)
}

/// The first line where two answer lists differ, for the message of a failed seed.
fn first_difference(left: &[String], right: &[String]) -> String {
    let at = left
        .iter()
        .zip(right)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| left.len().min(right.len()));
    format!(
        "line {at}: overlay {:?}, plain {:?}",
        left.get(at),
        right.get(at)
    )
}

/// Random sessions answer the same through the overlay as through the plain copy of the merged tree.
#[test]
fn random_sessions_answer_like_a_plain_folder() {
    // A run of 24 sessions takes about a minute; `GOBBO_ORACLE_SEEDS` runs more, for a long check before a release.
    let seeds = std::env::var("GOBBO_ORACLE_SEEDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(24);
    for seed in 1..=seeds {
        let (overlay, plain) = sessions(seed);
        assert_eq!(
            overlay,
            plain,
            "seed {seed}: {}",
            first_difference(&overlay, &plain)
        );
    }
}
