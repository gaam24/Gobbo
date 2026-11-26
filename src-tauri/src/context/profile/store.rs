//! The lifecycle shared by the two per-profile stores: the legacy `profile_data` files and the `user_data` sets. A
//! store answers the paths a profile materializes and where each copy lives; this module owns the owner file, the
//! switch order and the copy loops, so the two stores cannot drift apart.
//!
//! A switch over existing real files is journaled in the game's `state` folder. The journal is written after the old
//! owner's copy is captured and before the first real path changes, the owner file is written last, and the journal is
//! removed only when the switch is complete. A switch that starts with a journal restores the old owner first, so a
//! crash cannot leave the next switch capturing a mixed real folder into the old profile.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{copy, owner};
use crate::context::Data;
use crate::error::{AppError, io_at};
use crate::infra::json;

/// One materialized path and the copy that belongs to the profile.
#[derive(Clone)]
pub(super) struct MaterializedPath {
    pub(super) real: PathBuf,
    pub(super) store: PathBuf,
}

/// The owner file of a store: its name in `state/`, the error code for a file that cannot be read, and the switch
/// journal of this store.
#[derive(Clone, Copy)]
pub(super) struct OwnerFile {
    pub(super) name: &'static str,
    pub(super) invalid_code: &'static str,
    /// One journal per store, so a pending switch of one store is never overwritten by the other store's journal.
    pub(super) journal: &'static str,
}

impl OwnerFile {
    pub(super) fn read(self, data: &Data) -> Result<Option<String>, AppError> {
        owner::read_owner(data, self.name, self.invalid_code)
    }

    pub(super) fn write(self, data: &Data, profile: &str) -> Result<(), AppError> {
        owner::write_owner(data, self.name, profile)
    }
}

/// One per-profile store. `entries` is the only part a store owns; the owner file, the first seed and the switch
/// order live in this module.
pub(super) trait ProfileStore {
    /// The owner file in the game's `state` folder.
    fn owner(&self) -> OwnerFile;

    /// Whether the store applies to this game at all. A game with no `user_data` sets has nothing to switch.
    fn is_active(&self) -> bool {
        true
    }

    /// The paths materialized for `profile`, with the copy each one uses.
    fn entries(&self, data: &Data, profile: &str) -> Result<Vec<MaterializedPath>, AppError>;

    /// Runs before the owner check: creates the folders a redirect needs, or writes the target's manifest.
    fn prepare(&self, _data: &Data, _profile: &str) -> Result<(), AppError> {
        Ok(())
    }

    /// Whether the owner profile still exists. A profile deleted while it owned the store has nothing to save.
    fn exists(&self, _data: &Data, _profile: &str) -> Result<bool, AppError> {
        Ok(true)
    }

    /// The first run: the real files seed the profile.
    fn seed(&self, data: &Data, profile: &str) -> Result<(), AppError> {
        capture(&self.entries(data, profile)?)
    }
}

/// The phase a switch journal records. The owner file is the last write of a switch, so a journal whose owner is
/// already `to` means the switch finished, even when the phase update did not reach the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    /// The real paths are being restored; the owner still names `from`.
    Restoring,
    /// The owner was written; only removing the journal is left.
    Owned,
}

/// The journal of one store switch. It holds the store it belongs to and enough to undo a switch that stopped after it
/// started writing the real paths.
#[derive(Debug, Serialize, Deserialize)]
struct SwitchJournal {
    /// The owner file name, so a journal is never applied by the other store.
    store: String,
    from: String,
    to: String,
    phase: Phase,
}

/// Makes `target` the owner of `store`. The old owner's data is saved first; a first run seeds the target from the
/// real paths, and a later profile starts from its own store. A switch over existing real files is journaled, so an
/// interrupted restore is undone before the next switch captures anything.
pub(super) fn switch(store: &impl ProfileStore, data: &Data, target: &str) -> Result<(), AppError> {
    if !store.is_active() {
        return Ok(());
    }

    recover(store, data)?;
    store.prepare(data, target)?;
    let owner_file = store.owner();

    match owner_file.read(data)? {
        Some(current) if current == target => Ok(()),
        Some(current) if !store.exists(data, &current)? => {
            store.seed(data, target)?;
            owner_file.write(data, target)
        }
        Some(current) => switch_owner(store, data, &current, target),
        None => {
            store.seed(data, target)?;
            owner_file.write(data, target)
        }
    }
}

/// Switches from `from` to `to`, writing the owner last. The old owner's copy is captured before the journal exists
/// because no real path changes yet; a failure there leaves the previous switch intact. Once the journal is written, a
/// failed restore or owner write restores `from` before returning the error. A journal that cannot be removed after a
/// successful switch is harmless: the next entry sees the owner at `to` and only removes it.
fn switch_owner(
    store: &impl ProfileStore,
    data: &Data,
    from: &str,
    to: &str,
) -> Result<(), AppError> {
    let owner_file = store.owner();
    let from_entries = store.entries(data, from)?;
    capture(&from_entries)?;
    let to_entries = store.entries(data, to)?;
    let mut journal = SwitchJournal {
        store: owner_file.name.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        phase: Phase::Restoring,
    };
    write_journal(data, owner_file, &journal)?;

    if let Err(error) = restore(&to_entries, true) {
        return rollback(data, owner_file, &from_entries, &journal, error);
    }

    if let Err(error) = owner_file.write(data, to) {
        if owner_file.read(data)?.as_deref() == Some(to) {
            // The atomic replace went through and only the durability step failed. The owner is the last write, so
            // the switch is complete.
            let name = owner_file.name;
            crate::infra::log::warn(format!(
                "the owner of {name} was written, but the file system reported {error}"
            ));
            complete_journal(data, owner_file, &mut journal);
            return Ok(());
        }

        return rollback(data, owner_file, &from_entries, &journal, error);
    }

    complete_journal(data, owner_file, &mut journal);
    Ok(())
}

/// Restores `from`'s copy to the real paths after a failed switch and removes the journal. When the rollback cannot
/// finish, the journal stays behind and the next switch retries it.
fn rollback(
    data: &Data,
    owner_file: OwnerFile,
    from_entries: &[MaterializedPath],
    journal: &SwitchJournal,
    error: AppError,
) -> Result<(), AppError> {
    let name = owner_file.name;
    let from = &journal.from;
    let to = &journal.to;

    match restore(from_entries, true) {
        Ok(()) => {
            if let Err(cleanup) = remove_journal(data, owner_file) {
                crate::infra::log::warn(format!(
                    "{name} stayed at {from} after a failed switch to {to}, but its journal was left behind ({cleanup})"
                ));
            }
        }
        Err(rollback) => {
            crate::infra::log::warn(format!(
                "{name} could not return to {from} after a failed switch to {to} ({rollback}); the journal stays for the next switch"
            ));
        }
    }

    Err(error)
}

/// Records that the owner was written, then removes the journal. A failure here does not undo a completed switch: the
/// owner is already `to`, so the next entry only removes the journal.
fn complete_journal(data: &Data, owner_file: OwnerFile, journal: &mut SwitchJournal) {
    journal.phase = Phase::Owned;
    let name = owner_file.name;
    let to = &journal.to;

    if let Err(error) = write_journal(data, owner_file, journal) {
        crate::infra::log::warn(format!(
            "{name} switched to {to} but its journal could not record the phase ({error})"
        ));
    }

    if let Err(error) = remove_journal(data, owner_file) {
        crate::infra::log::warn(format!(
            "{name} switched to {to} but its journal was left behind ({error})"
        ));
    }
}

/// Finishes or undoes a switch that stopped with a journal. The owner is the last write of a switch, so when it already
/// names `to` the restore finished and only the journal is left. Otherwise the old owner's stored copy is put back,
/// which is the state the journal was written to protect, and the owner stays at `from`. A journal that cannot be read
/// is kept aside with a warning; it must not block the switch.
fn recover(store: &impl ProfileStore, data: &Data) -> Result<(), AppError> {
    let owner_file = store.owner();
    let path = journal_path(data, owner_file);
    let journal = match json::read_json::<SwitchJournal>(&path) {
        Ok(None) => return Ok(()),
        Ok(Some(journal)) => journal,
        Err(error) => {
            report_broken_journal(&path, error);
            return Ok(());
        }
    };

    if journal.store != owner_file.name {
        json::keep_broken(&path, &journal.store);
        return Ok(());
    }

    let current = owner_file.read(data)?;

    if current.as_deref() == Some(journal.to.as_str()) {
        return remove_journal(data, owner_file);
    }
    // A profile deleted while it owned the store has no copy to put back. The normal owner check below seeds the
    // target from the real files, exactly as it does without a journal.
    if store.exists(data, &journal.from)? {
        if journal.phase == Phase::Owned {
            // The phase says the owner was written, but the owner file does not name `to`: the write did not commit.
            let name = owner_file.name;
            let from = &journal.from;
            let to = &journal.to;
            crate::infra::log::warn(format!(
                "{name} lost its owner write to {to}; restoring {from}"
            ));
        }

        let entries = store.entries(data, &journal.from)?;
        restore(&entries, true)?;

        if current.as_deref() != Some(journal.from.as_str()) {
            owner_file.write(data, &journal.from)?;
        }
    }

    remove_journal(data, owner_file)
}

/// Reports a journal that cannot be read and keeps a damaged one aside, so the next switch does not report it again.
fn report_broken_journal(path: &Path, error: json::ReadError) {
    match error {
        json::ReadError::Parse(error) => json::keep_broken(path, &error),
        json::ReadError::Io(error) => crate::infra::log::warn(format!(
            "{} could not be read ({error}); the switch continues without it",
            path.display()
        )),
    }
}

fn journal_path(data: &Data, owner_file: OwnerFile) -> PathBuf {
    owner::path(data, owner_file.journal)
}

fn write_journal(
    data: &Data,
    owner_file: OwnerFile,
    journal: &SwitchJournal,
) -> Result<(), AppError> {
    json::write_json(&journal_path(data, owner_file), journal)
}

fn remove_journal(data: &Data, owner_file: OwnerFile) -> Result<(), AppError> {
    let path = journal_path(data, owner_file);
    crate::infra::fs::remove_file_if_exists(&path).map_err(io_at(&path))
}

/// Saves the owner's paths. Does nothing when no profile owns the store, or the owner is gone.
pub(super) fn capture_current(store: &impl ProfileStore, data: &Data) -> Result<(), AppError> {
    if !store.is_active() {
        return Ok(());
    }

    recover(store, data)?;
    let Some(current) = store.owner().read(data)? else {
        return Ok(());
    };

    if !store.exists(data, &current).unwrap_or(false) {
        return Ok(());
    }

    capture(&store.entries(data, &current)?)
}

/// Restores `profile`'s paths.
pub(super) fn restore_profile(
    store: &impl ProfileStore,
    data: &Data,
    profile: &str,
    remove_missing: bool,
) -> Result<(), AppError> {
    if !store.is_active() {
        return Ok(());
    }

    recover(store, data)?;
    restore(&store.entries(data, profile)?, remove_missing)
}

/// Copies every real path into its store. A missing real path removes the store entry.
pub(super) fn capture(entries: &[MaterializedPath]) -> Result<(), AppError> {
    for entry in entries {
        copy::capture(&entry.real, &entry.store)?;
    }

    Ok(())
}

/// Copies every store entry back to its real path. `remove_missing` removes a real path with no store entry.
pub(super) fn restore(entries: &[MaterializedPath], remove_missing: bool) -> Result<(), AppError> {
    for entry in entries {
        copy::restore(&entry.store, &entry.real, remove_missing)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::fs::fail_after;
    use crate::test_support::scratch;
    use std::fs;

    const OWNER: OwnerFile = OwnerFile {
        name: "fake_owner.json",
        invalid_code: "fake.owner_invalid",
        journal: "profile-switch-fake.json",
    };

    /// A store with two real folders, each copied to `store/<profile>/<name>`. The folders are real directories, so the
    /// copy failure injection inside `copy_dir` sees their files.
    struct FileStore {
        base: PathBuf,
        names: &'static [&'static str],
        exists: bool,
    }

    impl FileStore {
        fn new(base: PathBuf) -> Self {
            Self {
                base,
                names: &["a", "b"],
                exists: true,
            }
        }

        fn real(&self, name: &str) -> PathBuf {
            self.base.join("real").join(name)
        }

        fn stored(&self, profile: &str, name: &str) -> PathBuf {
            self.base.join("store").join(profile).join(name)
        }

        fn write_real(&self, name: &str, text: &str) {
            let path = self.real(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("save.txt"), text).unwrap();
        }

        fn read_real(&self, name: &str) -> String {
            fs::read_to_string(self.real(name).join("save.txt")).unwrap()
        }

        fn read_stored(&self, profile: &str, name: &str) -> String {
            fs::read_to_string(self.stored(profile, name).join("save.txt")).unwrap()
        }
    }

    impl ProfileStore for FileStore {
        fn owner(&self) -> OwnerFile {
            OWNER
        }

        fn entries(&self, _data: &Data, profile: &str) -> Result<Vec<MaterializedPath>, AppError> {
            Ok(self
                .names
                .iter()
                .map(|name| MaterializedPath {
                    real: self.real(name),
                    store: self.stored(profile, name),
                })
                .collect())
        }

        fn exists(&self, _data: &Data, _profile: &str) -> Result<bool, AppError> {
            Ok(self.exists)
        }
    }

    fn fixture(name: &str) -> (FileStore, Data) {
        let base = scratch!(name);
        let store = FileStore::new(base.join("files"));
        let data = Data::new(base.join("data"));
        store.write_real("a", "from-a");
        store.write_real("b", "from-b");
        switch(&store, &data, "p1").unwrap();
        (store, data)
    }

    #[test]
    fn a_deleted_owner_seeds_the_target_instead_of_restoring_it() {
        let (mut store, data) = fixture("store-owner-gone");
        store.exists = false;
        switch(&store, &data, "p2").unwrap();
        assert_eq!(store.read_stored("p2", "a"), "from-a");
        assert_eq!(store.read_stored("p2", "b"), "from-b");
        assert_eq!(OWNER.read(&data).unwrap().as_deref(), Some("p2"));
    }

    #[test]
    fn capture_current_does_nothing_without_an_owner() {
        let base = scratch!("store-no-owner");
        let store = FileStore::new(base.join("files"));
        let data = Data::new(base.join("data"));
        store.write_real("a", "from-a");
        store.write_real("b", "from-b");

        capture_current(&store, &data).unwrap();
        assert!(!store.stored("p1", "a").exists());
        assert!(!store.stored("p1", "b").exists());
        assert_eq!(store.read_real("a"), "from-a");
        assert_eq!(store.read_real("b"), "from-b");
    }

    #[test]
    fn capture_current_saves_the_owner_entries() {
        let (store, data) = fixture("store-capture");
        store.write_real("a", "to-a");
        store.write_real("b", "to-b");

        capture_current(&store, &data).unwrap();
        assert_eq!(store.read_stored("p1", "a"), "to-a");
        assert_eq!(store.read_stored("p1", "b"), "to-b");
    }

    #[test]
    fn a_failed_restore_rolls_back_to_the_old_owner_and_the_next_switch_succeeds() {
        let (store, data) = fixture("store-failed-restore");
        // Give p2 a stored copy, so restoring it copies both folders and the second copy can fail.
        switch(&store, &data, "p2").unwrap();
        store.write_real("a", "to-a");
        store.write_real("b", "to-b");
        switch(&store, &data, "p1").unwrap();

        // Capturing p1 copies two files; restoring p2 copies two more. Fail the second restore copy.
        fail_after(4);
        let error = switch(&store, &data, "p2").unwrap_err();
        fail_after(0);
        assert!(error.to_string().contains("injected"), "{error}");
        assert_eq!(OWNER.read(&data).unwrap().as_deref(), Some("p1"));
        assert_eq!(store.read_real("a"), "from-a");
        assert_eq!(store.read_real("b"), "from-b");
        assert!(!journal_path(&data, OWNER).exists());

        switch(&store, &data, "p2").unwrap();
        assert_eq!(OWNER.read(&data).unwrap().as_deref(), Some("p2"));
        assert_eq!(store.read_real("a"), "to-a");
        assert_eq!(store.read_real("b"), "to-b");
        assert!(!journal_path(&data, OWNER).exists());
    }

    #[test]
    fn a_journal_restores_the_old_owner_before_the_next_switch_captures_anything() {
        let (store, data) = fixture("store-journal-recover");
        // A crash in the middle of restoring p2: the owner is still p1 and the real files are mixed.
        store.write_real("a", "mixed-a");
        store.write_real("b", "mixed-b");
        write_journal(
            &data,
            OWNER,
            &SwitchJournal {
                store: OWNER.name.to_string(),
                from: "p1".to_string(),
                to: "p2".to_string(),
                phase: Phase::Restoring,
            },
        )
        .unwrap();

        switch(&store, &data, "p2").unwrap();
        assert_eq!(OWNER.read(&data).unwrap().as_deref(), Some("p2"));
        assert_eq!(
            store.read_stored("p1", "a"),
            "from-a",
            "the mixed real folder must not be captured into p1"
        );
        assert_eq!(store.read_stored("p1", "b"), "from-b");
        assert!(!journal_path(&data, OWNER).exists());
    }

    #[test]
    fn capture_current_recovers_a_pending_switch_before_saving() {
        let (store, data) = fixture("store-capture-recover");
        store.write_real("a", "mixed-a");
        store.write_real("b", "mixed-b");
        write_journal(
            &data,
            OWNER,
            &SwitchJournal {
                store: OWNER.name.to_string(),
                from: "p1".to_string(),
                to: "p2".to_string(),
                phase: Phase::Restoring,
            },
        )
        .unwrap();

        capture_current(&store, &data).unwrap();
        assert_eq!(
            store.read_stored("p1", "a"),
            "from-a",
            "the mixed real folder must not overwrite p1's copy"
        );
        assert_eq!(store.read_stored("p1", "b"), "from-b");
        assert_eq!(OWNER.read(&data).unwrap().as_deref(), Some("p1"));
        assert!(!journal_path(&data, OWNER).exists());
    }

    #[test]
    fn a_journal_whose_owner_was_already_written_only_removes_the_journal() {
        let (store, data) = fixture("store-journal-owned");
        switch(&store, &data, "p2").unwrap();
        assert!(!store.real("a").exists(), "p2 starts empty");
        // A crash between the owner write and the phase update.
        write_journal(
            &data,
            OWNER,
            &SwitchJournal {
                store: OWNER.name.to_string(),
                from: "p1".to_string(),
                to: "p2".to_string(),
                phase: Phase::Restoring,
            },
        )
        .unwrap();

        switch(&store, &data, "p2").unwrap();
        assert_eq!(OWNER.read(&data).unwrap().as_deref(), Some("p2"));
        assert!(!store.real("a").exists(), "a finished switch is not undone");
        assert!(!journal_path(&data, OWNER).exists());
    }

    #[test]
    fn a_completed_switch_leaves_no_journal_and_switching_again_is_a_no_op() {
        let (store, data) = fixture("store-no-journal");
        switch(&store, &data, "p2").unwrap();
        assert!(!journal_path(&data, OWNER).exists());

        switch(&store, &data, "p2").unwrap();
        assert_eq!(OWNER.read(&data).unwrap().as_deref(), Some("p2"));
        assert!(!store.real("a").exists(), "the second run restores nothing");
        assert!(!journal_path(&data, OWNER).exists());
    }

    #[test]
    fn a_damaged_journal_does_not_block_the_next_switch() {
        let (store, data) = fixture("store-journal-damaged");
        let path = journal_path(&data, OWNER);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json").unwrap();

        switch(&store, &data, "p2").unwrap();
        assert_eq!(OWNER.read(&data).unwrap().as_deref(), Some("p2"));
        assert!(!path.exists(), "the damaged journal is kept aside");
        assert!(path.with_extension("json.broken").exists());
    }
}
