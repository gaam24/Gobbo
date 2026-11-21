//! The game's own log files and the `.log` files the mods write. A definition names the first kind; the second kind is
//! discovered in the mod layer and the overwrite layer, so a mod or a new loader needs no definition change. The console
//! shows the tail, never the whole file, so a log that grows while the game runs costs one read of its end per refresh.

use std::path::{Path, PathBuf};

use gobbo_overlay_core::Layers;

use crate::context::Data;
use crate::context::load_settings;
use crate::deploy::overlay::Session;
use crate::deploy::target::deployment_root;
use crate::domain::pathkey::path_key;
use crate::games::{GameDef, LogKind, LogSource};
use crate::infra::clock;
use crate::infra::fs::{self, WalkErrors};
use crate::infra::text;
use crate::launch::report::GameLog;

/// How many lines of one log the console keeps.
const LOG_LINES: usize = 80;

/// A ceiling for one tail read, so a log without newlines cannot be walked from the start on every refresh.
const LOG_BYTES: u64 = 256 * 1024;

/// How many `.log` files the automatic scan of the mod layer and the overwrite layer lists. The newest are kept, so a
/// mod that ships an archive of old logs does not fill the console.
const MOD_LOG_LIMIT: usize = 50;

/// A smaller ceiling for a mod log: the console shows its end, and a modpack may hold many of them.
const MOD_LOG_BYTES: u64 = 64 * 1024;

/// The logs of `game` that exist for this installation, in definition order. A missing folder or file is simply left
/// out; a log that the game has not written yet must not make the console fail.
pub fn collect(data: &Data, game: &GameDef) -> Vec<GameLog> {
    if game.logs.is_empty() {
        return Vec::new();
    }

    let Some(root) = game_root(data) else {
        return Vec::new();
    };
    // The profile's state folder holds the overwrite layer, even after an undo removed the deployment marker. It is
    // the only place a game log from an earlier overlay run can be, so the console keeps reading it.
    let layers =
        deployed_layers(data, game, &root).unwrap_or_else(|| Layers::new(root.clone(), None, root));
    game.logs
        .iter()
        .flat_map(|source| source_logs(&layers, source))
        .collect()
}

/// Every `.log` file in the mod layer and the overwrite layer, newest first. These files belong to mods or to the
/// game's runtime, never to a definition path; a mod can add one without a definition change. A path already named by
/// `[[logs]]` is left out, because that source owns it.
pub fn collect_mod_logs(data: &Data, game: &GameDef, defined: &[GameLog]) -> Vec<GameLog> {
    let Some(root) = game_root(data) else {
        return Vec::new();
    };
    let Some(layers) = deployed_layers(data, game, &root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut bases = vec![(3, layers.upper.as_path())];

    if let Some(staging) = &layers.staging {
        bases.push((2, staging.as_path()));
    }

    for (rank, base) in bases {
        if !base.is_dir() {
            continue;
        }

        let _ = fs::walk(base, WalkErrors::Skip, &mut |_entry, relative, is_dir| {
            if !is_dir && is_log(relative) {
                push(
                    &mut found,
                    relative,
                    gobbo_overlay_core::join(base, relative),
                    rank,
                );
            }

            Ok(())
        });
    }

    found.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified_at));
    found
        .into_iter()
        .filter(|candidate| !is_defined(defined, &candidate.relative))
        .take(MOD_LOG_LIMIT)
        .filter_map(|candidate| read(candidate, LogKind::Mod, MOD_LOG_BYTES))
        .collect()
}

/// Whether a log the scan found is already named by a definition. Both paths go through the path-key rule, so a
/// separator or a letter case that differs between them does not list the same file twice.
fn is_defined(defined: &[GameLog], relative: &str) -> bool {
    let key = path_key(relative);
    defined.iter().any(|log| path_key(&log.path) == key)
}

fn game_root(data: &Data) -> Option<PathBuf> {
    let root = PathBuf::from(crate::context::load_settings(data).game_root.trim());
    (!root.as_os_str().is_empty()).then_some(root)
}

fn deployed_layers(data: &Data, game: &GameDef, game_root: &Path) -> Option<Layers> {
    let state = data.deployed_state().or_else(|| data.incomplete_state())?;
    let settings = load_settings(data);
    let deployment =
        deployment_root(data, game, &settings).unwrap_or_else(|_| game_root.to_path_buf());
    Some(Session::with_roots(&state, &deployment, game_root, Vec::new()).layers())
}

fn is_log(path: &str) -> bool {
    path.rsplit('.')
        .next()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("log"))
}

/// The newest files of one source. A file path gives one log; a folder gives up to `limit` logs, newest first.
fn source_logs(layers: &Layers, source: &LogSource) -> Vec<GameLog> {
    let path = source.path.trim();

    if path.is_empty() {
        return Vec::new();
    }
    // A pattern in the path itself names the folder it belongs to, for example `crash/Game.exe-*.txt`.
    let (folder, pattern) = if let Some((folder, name)) = path.rsplit_once('/') {
        if name.contains('*') {
            (folder, name.to_string())
        } else {
            (path, source.pattern.clone())
        }
    } else if path.contains('*') {
        (".", path.to_string())
    } else {
        (path, source.pattern.clone())
    };
    let is_folder = layer_bases(layers)
        .iter()
        .any(|(_, base)| gobbo_overlay_core::join(base, folder).is_dir());
    let mut candidates: Vec<Candidate> = Vec::new();

    if is_folder {
        collect_folder(layers, folder, &pattern, source.recursive, &mut candidates);
    } else {
        collect_file(layers, folder, &mut candidates);
    }

    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified_at));
    candidates
        .into_iter()
        .take(source.limit.max(1) as usize)
        .filter_map(|candidate| read(candidate, source.kind, LOG_BYTES))
        .collect()
}

/// One file found in a layer. `rank` keeps the upper layer over the mod layer over the game folder when the same
/// game-relative path exists in more than one of them.
struct Candidate {
    relative: String,
    path: PathBuf,
    rank: u8,
    modified_at: u64,
}

fn collect_file(layers: &Layers, relative: &str, found: &mut Vec<Candidate>) {
    for (rank, base) in layer_bases(layers) {
        let path = gobbo_overlay_core::join(base, relative);

        if path.is_file() {
            push(found, relative, path, rank);
        }
    }
}

fn collect_folder(
    layers: &Layers,
    folder: &str,
    pattern: &str,
    recursive: bool,
    found: &mut Vec<Candidate>,
) {
    let folder = folder.trim_end_matches('/');

    for (rank, base) in layer_bases(layers) {
        let dir = gobbo_overlay_core::join(base, folder);

        if !dir.is_dir() {
            continue;
        }

        if recursive {
            let _ = fs::walk(&dir, WalkErrors::Skip, &mut |_entry, relative, is_dir| {
                if !is_dir {
                    add_candidate(found, folder, relative, &dir, rank, pattern);
                }

                Ok(())
            });
        } else if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }

                let name = entry.file_name().to_string_lossy().into_owned();
                add_candidate(found, folder, &name, &dir, rank, pattern);
            }
        }
    }
}

fn add_candidate(
    found: &mut Vec<Candidate>,
    folder: &str,
    relative: &str,
    dir: &Path,
    rank: u8,
    pattern: &str,
) {
    let name = relative.rsplit('/').next().unwrap_or(relative);

    if !matches(name, pattern) {
        return;
    }

    let path = dir.join(relative);
    let full = if folder.is_empty() || folder == "." {
        relative.to_string()
    } else {
        format!("{folder}/{relative}")
    };
    push(found, &full, path, rank);
}

/// The layers from highest to lowest. A source that exists in more than one layer takes the highest one.
fn layer_bases(layers: &Layers) -> Vec<(u8, &Path)> {
    let mut bases = vec![(3, layers.upper.as_path())];

    if let Some(staging) = &layers.staging {
        bases.push((2, staging.as_path()));
    }

    bases.push((1, layers.root.as_path()));
    bases
}

fn push(found: &mut Vec<Candidate>, relative: &str, path: PathBuf, rank: u8) {
    let key = relative.replace('\\', "/");

    if let Some(existing) = found.iter_mut().find(|entry| entry.relative == key) {
        if rank > existing.rank {
            let modified_at = modified(&path);
            *existing = Candidate {
                relative: key,
                path,
                rank,
                modified_at,
            };
        }

        return;
    }

    let modified_at = modified(&path);
    found.push(Candidate {
        relative: key,
        path,
        rank,
        modified_at,
    });
}

fn modified(path: &Path) -> u64 {
    path.metadata()
        .and_then(|meta| meta.modified())
        .map_or(0, clock::seconds_since)
}

fn read(candidate: Candidate, kind: LogKind, max_bytes: u64) -> Option<GameLog> {
    let lines = text::tail_lines_with_limit(&candidate.path, LOG_LINES, max_bytes).ok()?;
    let modified_at = u32::try_from(candidate.modified_at).ok();
    Some(GameLog {
        kind: kind.as_str().to_string(),
        path: candidate.relative,
        lines,
        modified_at,
    })
}

/// A file name pattern with `*` as its only wildcard, compared without ASCII case. A path separator is not special:
/// the caller has already split the folder from the pattern.
fn matches(text: &str, pattern: &str) -> bool {
    fn inner(text: &[u8], pattern: &[u8]) -> bool {
        match pattern.split_first() {
            None => text.is_empty(),
            Some((b'*', rest)) => {
                inner(text, rest) || (!text.is_empty() && inner(&text[1..], pattern))
            }
            Some((expected, rest)) => {
                text.first()
                    .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
                    && inner(&text[1..], rest)
            }
        }
    }

    inner(text.as_bytes(), pattern.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::fs;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn a_pattern_matches_one_file_name_with_stars() {
        assert!(matches("red4ext-1.log", "red4ext-*.log"));
        assert!(matches("red4ext-1.log", "*.log"));
        assert!(matches("Redscript_rCURRENT.log", "*current.log"));
        assert!(!matches("red4ext-1.log", "*.txt"));
        assert!(!matches("red4ext-1.log", "red4ext-?.log"));
    }

    #[test]
    fn the_newest_file_of_a_folder_is_read_from_the_upper_layer() {
        let base = scratch!("game-logs");
        let root = base.join("game");
        let upper = base.join("upper");
        write(&root.join("red4ext/logs/old.log"), "old\n");
        write(&upper.join("red4ext/logs/new.log"), "new-1\nnew-2\n");
        let layers = Layers::new(root, None, upper);
        let source = LogSource {
            kind: crate::games::LogKind::Loader,
            path: "red4ext/logs".to_string(),
            pattern: "*.log".to_string(),
            limit: 1,
            recursive: true,
        };

        let logs = source_logs(&layers, &source);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].path, "red4ext/logs/new.log");
        assert_eq!(logs[0].lines, ["new-1", "new-2"]);
        assert_eq!(logs[0].kind, "loader");
    }

    #[test]
    fn the_definition_selects_the_logs_and_the_settings_name_the_game_folder() {
        let base = scratch!("game-log-definition");
        let game_root = base.join("game");
        write(&game_root.join("logs/one.log"), "one\n");
        write(&game_root.join("logs/two.txt"), "not a log\n");
        let data = Data::new(base);
        fs::write(
            data.config(),
            format!(
                r#"{{"game_root":{},"settings_dir":""}}"#,
                serde_json::to_string(&game_root.display().to_string()).unwrap()
            ),
        )
        .unwrap();
        let game = GameDef::parse(
            r#"
schema = 1
id = "test"
name = "Test"
[detect]
markers = ["game.exe"]
[[executables]]
name = "Game"
path = "game.exe"
[deploy]
[[logs]]
kind = "script"
path = "logs"
pattern = "*.log"
limit = 2
"#,
        )
        .unwrap();

        let logs = collect(&data, &game);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].path, "logs/one.log");
        assert_eq!(logs[0].lines, ["one"]);
    }

    #[test]
    fn every_log_of_the_mod_and_overwrite_layers_is_found_without_a_definition() {
        let base = scratch!("game-mod-logs");
        let game_root = base.join("game");
        let state = base.join("state/p1");
        write(
            &state.join("overwrite/red4ext/logs/red4ext.log"),
            "loader\n",
        );
        write(
            &state.join("overwrite/r6/logs/redscript_rCURRENT.log"),
            "compile\n",
        );
        write(&state.join("overwrite/readme.txt"), "not a log\n");
        write(
            &state.join("mods/red4ext/plugins/Codeware/Codeware-1.log"),
            "plugin\n",
        );
        fs::create_dir_all(&state).unwrap();
        fs::write(state.join("journal.jsonl"), "").unwrap();
        let data = Data::new(base);
        fs::write(
            data.config(),
            format!(
                r#"{{"game_root":{},"settings_dir":""}}"#,
                serde_json::to_string(&game_root.display().to_string()).unwrap()
            ),
        )
        .unwrap();
        let defined = vec![GameLog {
            kind: "script".to_string(),
            path: "r6/logs/redscript_rCURRENT.log".to_string(),
            lines: Vec::new(),
            modified_at: None,
        }];

        let game = crate::test_support::games::slot();
        let logs = collect_mod_logs(&data, &game, &defined);
        let paths: Vec<&str> = logs.iter().map(|log| log.path.as_str()).collect();
        assert!(paths.contains(&"red4ext/logs/red4ext.log"));
        assert!(paths.contains(&"red4ext/plugins/Codeware/Codeware-1.log"));
        assert!(!paths.contains(&"r6/logs/redscript_rCURRENT.log"));
        assert!(!paths.contains(&"readme.txt"));
        assert_eq!(logs[0].kind, "mod");
    }

    #[test]
    fn a_defined_log_is_recognized_whatever_its_separator_or_case() {
        let defined = vec![GameLog {
            kind: "script".to_string(),
            path: "r6\\logs\\Redscript_rCURRENT.log".to_string(),
            lines: Vec::new(),
            modified_at: None,
        }];

        assert!(is_defined(&defined, "r6/logs/redscript_rcurrent.log"));
        assert!(is_defined(&defined, "r6/logs/Redscript_rCURRENT.log"));
        assert!(!is_defined(&defined, "r6/logs/other.log"));
    }

    #[test]
    fn a_single_log_path_is_read_without_a_pattern() {
        let base = scratch!("game-log-file");
        let root = base.join("game");
        write(&root.join("r6/logs/redscript_rCURRENT.log"), "done\n");
        let layers = Layers::new(root, None, base.join("upper"));
        let source = LogSource {
            kind: crate::games::LogKind::Script,
            path: "r6/logs/redscript_rCURRENT.log".to_string(),
            pattern: "*.log".to_string(),
            limit: 1,
            recursive: true,
        };

        let logs = source_logs(&layers, &source);
        assert_eq!(logs[0].path, "r6/logs/redscript_rCURRENT.log");
        assert_eq!(logs[0].lines, ["done"]);
    }
}
