//! The programs a game folder offers, and the order they are offered in.

use std::cmp::Ordering;
use std::path::Path;

use super::super::ExecutableSuggestion;
use super::folder::{Entry, walk};

/// How many folders below the game folder the program search reads: `Engine/Binaries/Win64` is three.
const PROGRAM_DEPTH: usize = 3;

/// The shorter of two names must have this many characters before one containing the other counts as a match. A short
/// word such as `game` is part of many names and would match too much.
const PARTIAL_NAME_MIN: usize = 6;

/// Words that mark a helper program rather than the game: a crash reporter, a checksum or verify tool, an updater,
/// patcher, setup, configuration, benchmark or editor. A helper is offered after the other programs, never hidden;
/// `check` also covers `checksum` and `checker`.
const HELPER_WORDS: &[&str] = &[
    "crash",
    "report",
    "helper",
    "check",
    "verify",
    "update",
    "patch",
    "setup",
    "config",
    "benchmark",
    "editor",
];

/// Installers, crash handlers and setup programs that are not the game.
const SKIP_EXE_PREFIXES: &[&str] = &[
    "unins",
    "unitycrashhandler",
    "vc_redist",
    "vcredist",
    "dxsetup",
    "dotnetfx",
    "ue4prereqsetup",
    "ue5prereqsetup",
];

/// The programs of a game folder, best first. The folder name and the game's display name are the names a program may
/// match.
pub(super) fn executables(folder: &Path, name: &str) -> Vec<ExecutableSuggestion> {
    let folder_name = folder
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let names = [folder_name.as_str(), name];
    let mut programs = walk(folder, PROGRAM_DEPTH, |entry, is_dir| {
        !is_dir && is_program(entry)
    });
    programs.sort_by(|left, right| program_order(left, right, &names));
    programs
        .into_iter()
        .map(|program| ExecutableSuggestion {
            name: program.name,
            path: program.path,
        })
        .collect()
}

/// Whether a file name is a program the game could start: an `.exe` that is not an installer or a crash handler.
fn is_program(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    Path::new(&lower)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        && !SKIP_EXE_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
}

/// The order the programs are offered in, best first: a program whose name is the game's name comes before one whose
/// name contains it, and both before the rest; a helper program comes after the others; then the shallower folder, the
/// larger file and the path.
fn program_order(left: &Entry, right: &Entry, names: &[&str]) -> Ordering {
    name_rank(&left.name, names)
        .cmp(&name_rank(&right.name, names))
        .then_with(|| is_helper(&left.name).cmp(&is_helper(&right.name)))
        .then_with(|| left.depth.cmp(&right.depth))
        .then_with(|| right.size.cmp(&left.size))
        .then_with(|| left.path.cmp(&right.path))
}

/// How well a program's name matches the game's names: 0 when the name is one of them, 1 when one contains the other
/// (and the shorter has at least `PARTIAL_NAME_MIN` characters), 2 otherwise.
fn name_rank(file_name: &str, names: &[&str]) -> u8 {
    let program = comparable(file_stem(file_name));
    let mut rank = 2;

    for name in names.iter().map(|name| comparable(name)) {
        if program.is_empty() || name.is_empty() {
            continue;
        }

        if program == name {
            return 0;
        }

        // The longer name must contain the shorter one, and the shorter one must be long enough to mean something.
        let (longer, shorter) = if program.chars().count() >= name.chars().count() {
            (program.as_str(), name.as_str())
        } else {
            (name.as_str(), program.as_str())
        };

        if shorter.chars().count() >= PARTIAL_NAME_MIN && longer.contains(shorter) {
            rank = 1;
        }
    }

    rank
}

/// Whether a program is a helper, by the words in its name (see `HELPER_WORDS`).
fn is_helper(file_name: &str) -> bool {
    let key = comparable(file_stem(file_name));
    HELPER_WORDS.iter().any(|word| key.contains(word))
}

/// A file name without its last extension: `Game Setup.exe` gives `Game Setup`.
fn file_stem(file_name: &str) -> &str {
    file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem)
}

/// A name as the program search compares it: its letters and digits in lower case. Case, spaces, apostrophes and other
/// punctuation do not count, so `Example's Game` and `ExamplesGame` are the same name.
fn comparable(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
