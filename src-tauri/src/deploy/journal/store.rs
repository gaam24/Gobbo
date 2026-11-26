use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use crate::context::data::DEPLOYMENT_FILE;
use crate::deploy::error::DeployError;
use crate::domain::deployment_record::DeploymentRecord;
use crate::error::FileError;

use super::entry::JournalOp;

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
    pub fn append(&self, entry: &JournalOp) -> Result<(), DeployError> {
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
        mut visit: impl FnMut(JournalOp) -> Result<(), DeployError>,
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

            match serde_json::from_str::<JournalOp>(text) {
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
            if matches!(entry, JournalOp::LayerPrepared { .. }) {
                found = true;
            }

            Ok(())
        })?;
        Ok(found)
    }

    /// All entries in write order. No journal means an empty list.
    pub fn read(&self) -> Result<Vec<JournalOp>, DeployError> {
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
