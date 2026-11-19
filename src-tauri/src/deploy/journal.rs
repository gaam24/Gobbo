//! The deployment journal in JSON Lines format, one entry per change on disk. The entry is written before the change,
//! so after an interruption `undo` knows what may have reached the disk.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::context::data::DEPLOYMENT_FILE;
use crate::deploy::DeployError;
use crate::domain::deployment::DeploymentRecord;
use crate::domain::pathkey;
use crate::error::FileError;

/// One change the journal records before it reaches the disk, so undo knows what may have changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Entry {
    /// A folder created by Gobbo.
    DirCreated { path: PathBuf },
    /// A file deployed from the library.
    FileDeployed { path: PathBuf },
    /// The sections Gobbo changed in the settings file, and whether the file existed before. Undo puts back only these.
    SettingsSections {
        path: PathBuf,
        existed: bool,
        touched: Vec<crate::deploy::settings::Touched>,
    },
    /// An overlay session: the mod layer in `mods` stays after undo, for the next deployment of this profile.
    /// `overwrite`, which holds the game's writes, stays too.
    LayerPrepared { mods: PathBuf, overwrite: PathBuf },
}

/// The places one deployment may touch. A journal is data on disk that anyone can edit, so undo checks every entry
/// against these before it removes or writes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalRoots {
    /// The profile's state folder: the journal, the overlay layer and the profile's user-data layers.
    pub state: PathBuf,
    /// The deployment root, where `FileDeployed` and `DirCreated` put the game's paths.
    pub deployment: PathBuf,
    /// The game's order file, when the game has one. A redirect writes it in the profile state instead.
    pub settings: Option<PathBuf>,
}

impl JournalRoots {
    /// The roots of one deployment: the profile state, the deployment root and the game's order file.
    pub const fn new(state: PathBuf, deployment: PathBuf, settings: Option<PathBuf>) -> Self {
        Self {
            state,
            deployment,
            settings,
        }
    }

    /// The same game roots for another profile's state folder. The overlay layers and the settings redirects are per
    /// profile, so a make-room undo of another profile uses that profile's state.
    pub fn at_state(&self, state: PathBuf) -> Self {
        Self {
            state,
            deployment: self.deployment.clone(),
            settings: self.settings.clone(),
        }
    }

    /// The first path of `entry` that lies outside the roots, or `None` when every path is allowed. A path with `..`,
    /// a relative path, a drive or UNC prefix that escapes, a path in another game or settings folder, or a path in
    /// the profile state outside the overlay layer folders is refused.
    pub fn unsafe_path<'a>(&self, entry: &'a Entry) -> Option<&'a Path> {
        match entry {
            Entry::DirCreated { path } | Entry::FileDeployed { path } => {
                (!self.allows_deployed(path)).then_some(path)
            }
            Entry::SettingsSections { path, .. } => (!self.allows_settings(path)).then_some(path),
            Entry::LayerPrepared { mods, overwrite } => {
                if !is_layer_path(&self.state.join("mods"), mods) {
                    Some(mods)
                } else if !is_layer_path(&self.state.join("overwrite"), overwrite) {
                    Some(overwrite)
                } else {
                    None
                }
            }
        }
    }

    fn allows_deployed(&self, path: &Path) -> bool {
        safe_journal_path(path)
            && (pathkey::is_inside(&self.deployment, path)
                // An overlay step journals its outputs in the game session's upper layer, which is `overwrite/`.
                // The mod layer is Gobbo's too, so the same entry kinds may name it.
                || pathkey::is_inside(&self.state.join("overwrite"), path)
                || pathkey::is_inside(&self.state.join("mods"), path))
    }

    fn allows_settings(&self, path: &Path) -> bool {
        let Some(settings) = &self.settings else {
            return false;
        };
        if !safe_journal_path(path) {
            return false;
        }
        if is_same_path(settings, path) {
            return true;
        }
        // A redirect writes the order file in `<state>/userdata/<set>/upper/`; the copy store and the base folder are
        // not settings targets.
        let userdata = self.state.join("userdata");
        let Some(relative) = pathkey::relative_to(&userdata, path) else {
            return false;
        };
        let Some(Component::Normal(set)) = relative.components().next() else {
            return false;
        };
        pathkey::is_inside(&userdata.join(set).join("upper"), path)
    }
}

/// A path a journal may name: absolute, with no `.` or `..`, and with only a normal disk or UNC prefix on Windows.
/// A relative path would resolve against the process working directory, and `..` would escape a lexical root check.
fn safe_journal_path(path: &Path) -> bool {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return false;
    }
    path.components().all(|component| match component {
        Component::Normal(_) | Component::RootDir => true,
        Component::Prefix(prefix) => {
            matches!(prefix.kind(), Prefix::Disk(_) | Prefix::UNC(_, _))
        }
        Component::CurDir | Component::ParentDir => false,
    })
}

/// Whether `path` is exactly `root`, following the platform's case rule.
fn is_same_path(root: &Path, path: &Path) -> bool {
    pathkey::relative_to(root, path).is_some_and(|relative| relative.as_os_str().is_empty())
}

/// A layer folder must be exactly the one Gobbo owns under the profile's state folder; no child may be named.
fn is_layer_path(root: &Path, path: &Path) -> bool {
    safe_journal_path(path) && is_same_path(root, path)
}

/// The journal of one apply: the entry file and the completion marker next to it. The marker is written only after
/// every file and the settings are in place, so a journal alone is an interrupted apply and not a deployment the game
/// may use.
pub struct Journal {
    path: PathBuf,
    /// The open journal file, from the first `append` until `remove`, or until the journal is dropped. The buffer is
    /// reused by every entry, so a deployment of thousands of files does not allocate a new line each time.
    writer: Mutex<Option<Writer>>,
}

struct Writer {
    file: File,
    buffer: Vec<u8>,
    /// Entries written since the last `sync_data`. A long apply syncs in batches, so a power loss loses at most the
    /// tail of the journal, not the whole deployment.
    unsynced: u32,
}

impl Journal {
    /// A journal at `path`, with no file open until the first entry is appended.
    pub const fn new(path: PathBuf) -> Self {
        Self {
            path,
            writer: Mutex::new(None),
        }
    }

    /// Whether the journal file is on disk, whether or not this handle has opened it.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Whether the apply that owns this journal reached its end. A missing marker is not complete, and neither is a
    /// marker this build cannot trust: an empty, damaged or newer one leaves the apply interrupted, for undo or the
    /// next apply to repair.
    pub fn is_complete(&self) -> bool {
        self.read_complete().is_some()
    }

    /// The completion marker when it is a record this build understands. A missing, empty, damaged or newer marker is
    /// `None`; the file itself is left in place for diagnosis.
    pub fn read_complete(&self) -> Option<DeploymentRecord> {
        crate::infra::deployment::read_complete(&self.complete_path())
    }

    /// The marker next to the journal. It is the deployed flag a status reads.
    pub fn complete_path(&self) -> PathBuf {
        self.path.with_file_name(DEPLOYMENT_FILE)
    }

    /// Writes the completion marker. The caller writes it only after the last file and the settings are in place.
    pub fn complete(&self, record: &DeploymentRecord) -> Result<(), DeployError> {
        self.sync()?;
        let bytes = serde_json::to_vec(record).map_err(DeployError::Json)?;
        crate::infra::store::write_atomic(&self.complete_path(), &bytes).map_err(DeployError::Io)
    }

    /// Appends one entry. The file is opened by the first entry and kept open, so a deployment of thousands of files
    /// does not open the journal thousands of times, which on Windows each open pays for in a virus scan. Each entry is
    /// written straight to the operating system, so a crash of this process loses nothing that was appended.
    pub fn append(&self, entry: &Entry) -> Result<(), DeployError> {
        let mut writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        if writer.is_none() {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .map_err(|e| FileError::new(&self.path, e))?;
            *writer = Some(Writer {
                file,
                buffer: Vec::with_capacity(256),
                unsynced: 0,
            });
        }
        // The file was opened just above, so the `None` case cannot happen.
        writer.as_mut().map_or(Ok(()), |writer| {
            writer.buffer.clear();
            serde_json::to_writer(&mut writer.buffer, entry).map_err(DeployError::Json)?;
            writer.buffer.push(b'\n');
            writer
                .file
                .write_all(&writer.buffer)
                .map_err(|e| FileError::new(&self.path, e))?;
            writer.unsynced += 1;
            if writer.unsynced >= 64 {
                writer
                    .file
                    .sync_data()
                    .map_err(|e| FileError::new(&self.path, e))?;
                writer.unsynced = 0;
            }
            Ok(())
        })
    }

    /// Flushes the journal to the device. Called before the completion marker, so the journal is durable before the
    /// status can call the deployment applied.
    pub fn sync(&self) -> Result<(), DeployError> {
        let mut writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(writer) = writer.as_mut() {
            writer
                .file
                .sync_data()
                .map_err(|e| FileError::new(&self.path, e))?;
            writer.unsynced = 0;
        }
        drop(writer);
        Ok(())
    }

    /// Calls `visit` for every entry in write order, without keeping the whole journal in memory. No journal means no
    /// calls. Undoing still reads them all (it needs the reverse order), but a check of what the journal owns only
    /// keeps the paths it needs.
    pub fn for_each(
        &self,
        mut visit: impl FnMut(Entry) -> Result<(), DeployError>,
    ) -> Result<(), DeployError> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(FileError::new(&self.path, e).into()),
        };
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|e| FileError::new(&self.path, e))?;
            if read == 0 {
                break;
            }
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            match serde_json::from_str::<Entry>(text) {
                Ok(entry) => visit(entry)?,
                Err(error) => {
                    // A crash in the middle of `write_all` can leave a partial last line. An entry from a newer build
                    // can name an `op` this build does not know. Neither may make the journal unusable: skip the line,
                    // log it, and let undo or the next apply repair what it can.
                    let at_end = reader.fill_buf().map_or(true, <[u8]>::is_empty);
                    crate::infra::log::warn(format!(
                        "journal {}: skipping unreadable entry{}: {error}",
                        self.path.display(),
                        if at_end { " at the end" } else { "" }
                    ));
                    if at_end {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Whether this journal prepared an overlay layer. A missing journal is not an overlay session.
    pub fn has_layer_prepared(&self) -> Result<bool, DeployError> {
        let mut found = false;
        self.for_each(|entry| {
            if matches!(entry, Entry::LayerPrepared { .. }) {
                found = true;
            }
            Ok(())
        })?;
        Ok(found)
    }

    /// All entries in write order. No journal means an empty list.
    pub fn read(&self) -> Result<Vec<Entry>, DeployError> {
        let mut entries = Vec::new();
        self.for_each(|entry| {
            entries.push(entry);
            Ok(())
        })?;
        Ok(entries)
    }

    /// Deletes the journal and its completion marker. The open file is closed first: Windows does not delete a file
    /// that is still open here.
    pub fn remove(&self) -> Result<(), DeployError> {
        *self.writer.lock().unwrap_or_else(PoisonError::into_inner) = None;
        for path in [self.complete_path(), self.path.clone()] {
            crate::infra::fs::remove_file_if_exists(&path)
                .map_err(|error| FileError::new(&path, error))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::fs;

    #[test]
    fn entries_are_read_back_in_order_and_the_journal_is_removed_after_appending() {
        let dir = scratch!("journal");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let journal = Journal::new(dir.join("journal.jsonl"));

        journal
            .append(&Entry::DirCreated {
                path: dir.join("a"),
            })
            .unwrap();
        journal
            .append(&Entry::FileDeployed {
                path: dir.join("a/x"),
            })
            .unwrap();
        let entries = journal.read().unwrap();
        assert_eq!(
            entries,
            vec![
                Entry::DirCreated {
                    path: dir.join("a")
                },
                Entry::FileDeployed {
                    path: dir.join("a/x")
                },
            ]
        );

        // The file is still open from the appends; removing it must work on Windows too.
        journal.remove().unwrap();
        assert!(!journal.exists());
    }

    #[test]
    fn the_completion_marker_is_written_and_removed_with_the_journal() {
        let dir = scratch!("journal-complete");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let journal = Journal::new(dir.join("journal.jsonl"));
        journal
            .append(&Entry::FileDeployed {
                path: dir.join("a/x"),
            })
            .unwrap();
        assert!(!journal.is_complete());

        journal
            .complete(&DeploymentRecord {
                schema_version: crate::domain::STATE_SCHEMA,
                files: 1,
                plan_hash: "abc".to_string(),
            })
            .unwrap();
        assert!(journal.is_complete());
        assert!(journal.complete_path().is_file());
        assert_eq!(
            journal.read_complete().map(|record| record.schema_version),
            Some(crate::domain::STATE_SCHEMA)
        );

        journal.remove().unwrap();
        assert!(!journal.exists());
        assert!(!journal.is_complete());
        assert!(!journal.complete_path().exists());
    }

    #[test]
    fn an_empty_or_damaged_marker_is_not_complete_and_is_kept() {
        for (name, content) in [
            ("journal-marker-empty", ""),
            ("journal-marker-corrupt", "{ not json"),
            ("journal-marker-no-fields", "{}"),
        ] {
            let dir = scratch!(name);
            let journal = Journal::new(dir.join("journal.jsonl"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(journal.complete_path(), content).unwrap();

            assert!(!journal.is_complete(), "{name}");
            assert!(journal.read_complete().is_none(), "{name}");
            assert!(
                journal.complete_path().is_file(),
                "the damaged marker is kept for diagnosis: {name}"
            );
        }
    }

    #[test]
    fn a_marker_from_a_newer_build_is_not_complete() {
        let dir = scratch!("journal-marker-newer");
        let journal = Journal::new(dir.join("journal.jsonl"));
        fs::create_dir_all(&dir).unwrap();
        let marker = format!(
            r#"{{"schema_version":{},"method":"hardlink","files":1,"plan_hash":"abc"}}"#,
            crate::domain::STATE_SCHEMA + 1
        );
        fs::write(journal.complete_path(), marker).unwrap();

        assert!(!journal.is_complete());
        assert!(journal.read_complete().is_none());
    }

    #[test]
    fn an_old_marker_without_a_schema_version_is_complete() {
        let dir = scratch!("journal-marker-old");
        let journal = Journal::new(dir.join("journal.jsonl"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            journal.complete_path(),
            r#"{"method":"hardlink","files":1,"plan_hash":"abc"}"#,
        )
        .unwrap();

        let record = journal.read_complete().unwrap();
        assert_eq!(record.schema_version, crate::domain::STATE_SCHEMA);
        assert!(record.is_supported());
        assert!(journal.is_complete());
    }

    #[test]
    fn a_missing_journal_has_no_entries_and_can_be_removed_twice() {
        let dir = scratch!("journal-missing");
        let journal = Journal::new(dir.join("journal.jsonl"));
        assert!(!journal.exists());
        journal
            .for_each(|_| Ok::<(), crate::deploy::DeployError>(()))
            .unwrap();
        assert!(!journal.has_layer_prepared().unwrap());
        journal.remove().unwrap();
        journal.remove().unwrap();
    }

    #[test]
    fn a_journal_exists_after_an_append_and_finds_its_layer_entry() {
        let dir = scratch!("journal-layer");
        let journal = Journal::new(dir.join("journal.jsonl"));
        journal
            .append(&Entry::FileDeployed {
                path: dir.join("game/x"),
            })
            .unwrap();
        assert!(journal.exists());
        assert!(!journal.has_layer_prepared().unwrap());

        journal
            .append(&Entry::LayerPrepared {
                mods: dir.join("state/mods"),
                overwrite: dir.join("state/overwrite"),
            })
            .unwrap();
        assert!(journal.has_layer_prepared().unwrap());
    }

    #[test]
    fn a_torn_last_line_is_ignored_and_the_earlier_entries_survive() {
        let dir = scratch!("journal-torn");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.jsonl");
        fs::write(
            &path,
            "{\"op\":\"dir_created\",\"path\":\"a\"}\n{\"op\":\"file_dep",
        )
        .unwrap();

        let entries = Journal::new(path).read().unwrap();
        assert_eq!(entries, vec![Entry::DirCreated { path: "a".into() }]);
    }

    #[test]
    fn an_unknown_entry_is_skipped_and_the_known_entries_survive() {
        let dir = scratch!("journal-unknown");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.jsonl");
        fs::write(
            &path,
            "{\"op\":\"from_a_newer_build\",\"path\":\"a\"}\n{\"op\":\"dir_created\",\"path\":\"b\"}\n",
        )
        .unwrap();

        let entries = Journal::new(path).read().unwrap();
        assert_eq!(entries, vec![Entry::DirCreated { path: "b".into() }]);
    }

    fn roots(base: &Path) -> JournalRoots {
        JournalRoots::new(
            base.join("state"),
            base.join("game"),
            Some(base.join("settings/mods.settings")),
        )
    }

    #[test]
    fn the_roots_allow_every_path_a_valid_apply_writes() {
        let base = scratch!("journal-roots-valid");
        let roots = roots(&base);
        let entries = [
            Entry::DirCreated {
                path: base.join("game/Mods/modA"),
            },
            Entry::FileDeployed {
                path: base.join("game/Mods/modA/content/x.ws"),
            },
            Entry::DirCreated {
                path: base.join("state/overwrite/cache"),
            },
            Entry::FileDeployed {
                path: base.join("state/overwrite/cache/final.bin"),
            },
            Entry::FileDeployed {
                path: base.join("state/mods/Mods/modA/content/x.ws"),
            },
            Entry::SettingsSections {
                path: base.join("settings/mods.settings"),
                existed: true,
                touched: Vec::new(),
            },
            Entry::SettingsSections {
                path: base.join("state/userdata/documents/upper/mods.settings"),
                existed: false,
                touched: Vec::new(),
            },
            Entry::LayerPrepared {
                mods: base.join("state/mods"),
                overwrite: base.join("state/overwrite"),
            },
        ];
        for entry in &entries {
            assert_eq!(roots.unsafe_path(entry), None, "{entry:?}");
        }
    }

    #[test]
    fn a_relative_dotdot_or_absolute_escape_is_unsafe() {
        let base = scratch!("journal-roots-escape");
        let roots = roots(&base);
        let outside = base.parent().unwrap().join("outside");
        let entries = [
            Entry::FileDeployed {
                path: PathBuf::from("relative/path"),
            },
            Entry::DirCreated {
                path: base.join("game/../outside"),
            },
            Entry::FileDeployed {
                path: outside.clone(),
            },
            Entry::FileDeployed {
                path: PathBuf::from(r"\\server\share\outside"),
            },
            Entry::FileDeployed {
                path: PathBuf::from(r"C:\Windows\System32"),
            },
        ];
        for entry in &entries {
            assert!(
                roots.unsafe_path(entry).is_some(),
                "an escaping path must be unsafe: {entry:?}"
            );
        }
        assert_eq!(
            roots.unsafe_path(&Entry::FileDeployed {
                path: outside.clone()
            }),
            Some(outside.as_path())
        );
    }

    #[test]
    fn a_path_in_another_games_folder_is_unsafe() {
        let base = scratch!("journal-roots-game");
        let roots = roots(&base);
        let other = base.join("other-game/Mods/modA/content/x.ws");
        assert_eq!(
            roots.unsafe_path(&Entry::FileDeployed {
                path: other.clone()
            }),
            Some(other.as_path())
        );
    }

    #[test]
    fn a_state_file_outside_the_layer_folders_is_unsafe() {
        let base = scratch!("journal-roots-state");
        let roots = JournalRoots::new(
            base.join("data/state/p1"),
            base.join("game"),
            Some(base.join("settings/mods.settings")),
        );
        for path in [
            base.join("data/state/p1/journal.jsonl"),
            base.join("data/state/p1/deployment.json"),
            base.join("data/state/p1/userdata/documents/upper/x.sav"),
            base.join("data/state/profiles.json"),
            base.join("data/state/p2/overwrite/x.sav"),
            base.join("data/state/p2/mods/Mods/modA/x.ws"),
        ] {
            assert_eq!(
                roots.unsafe_path(&Entry::FileDeployed { path: path.clone() }),
                Some(path.as_path()),
                "a state file is not a deployment path"
            );
        }
        assert_eq!(
            roots.unsafe_path(&Entry::FileDeployed {
                path: base.join("data/state/p1/overwrite/x.sav"),
            }),
            None
        );
    }

    #[test]
    fn a_settings_path_outside_the_settings_target_is_unsafe() {
        let base = scratch!("journal-roots-settings");
        let roots = roots(&base);
        for path in [
            base.join("other/mods.settings"),
            base.join("game/mods.settings"),
            base.join("state/other.json"),
            base.join("state/userdata/documents/copy/mods.settings"),
            base.join("state/userdata/documents/base/mods.settings"),
            base.join("state/userdata/upper/mods.settings"),
        ] {
            assert_eq!(
                roots.unsafe_path(&Entry::SettingsSections {
                    path: path.clone(),
                    existed: true,
                    touched: Vec::new(),
                }),
                Some(path.as_path())
            );
        }
        assert_eq!(
            roots.unsafe_path(&Entry::LayerPrepared {
                mods: base.join("other/mods"),
                overwrite: base.join("state/overwrite"),
            }),
            Some(base.join("other/mods").as_path())
        );
    }
}
