//! Copying files in chunks: a progress meter, a copy that hashes in the same pass, and a digest of an existing file.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use faster_hex::hex_string;
use sha2::Digest;

/// Size of a chunk when copying and hashing. Progress is reported after each chunk.
const CHUNK: usize = 1 << 20;

/// Copies a file while hashing it, so an archive is read once to be stored and given its sum. Returns the hash as hex.
/// A failed copy leaves no file at `target`.
pub fn hash_and_copy<D: Digest>(
    source: &Path,
    target: &Path,
    progress: &mut dyn FnMut(u32),
) -> io::Result<String> {
    copy_file::<D>(source, target, None, progress)
}

/// `hash_and_copy`, stopping with `io::ErrorKind::Interrupted` at the next chunk after `cancel` is set. A stopped copy
/// leaves no file at `target`, so a cancelled import does not keep a partial archive.
pub fn hash_and_copy_cancellable<D: Digest>(
    source: &Path,
    target: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u32),
) -> io::Result<String> {
    copy_file::<D>(source, target, Some(cancel), progress)
}

/// Opens a source file and copies it while hashing, with or without a cancel flag.
fn copy_file<D: Digest>(
    source: &Path,
    target: &Path,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(u32),
) -> io::Result<String> {
    let mut from = File::open(source)?;
    let total = from.metadata()?.len();
    hash_and_copy_inner::<D, _>(&mut from, target, total, cancel, progress)
        .map(|(digest, _)| digest)
}

/// `hash_and_copy` for a source that is not a file on disk, such as an entry of a pack: copies the reader to `target`
/// while hashing it and returns the hex digest and the bytes copied. `total` is the size the source promises, for the
/// progress; it may be 0 when the size is unknown. The caller caps the copy by handing in a limited reader. A failed
/// copy leaves no file at `target`.
pub fn hash_and_copy_reader<D: Digest, R: Read + ?Sized>(
    source: &mut R,
    target: &Path,
    total: u64,
    progress: &mut dyn FnMut(u32),
) -> io::Result<(String, u64)> {
    hash_and_copy_inner::<D, R>(source, target, total, None, progress)
}

fn hash_and_copy_inner<D: Digest, R: Read + ?Sized>(
    source: &mut R,
    target: &Path,
    total: u64,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(u32),
) -> io::Result<(String, u64)> {
    let mut to = File::create(target)?;
    let mut hasher = D::new();
    let mut reporter = Reporter::new(total, progress);
    let result = {
        let mut writer = HashingWriter {
            to: &mut to,
            hasher: &mut hasher,
        };
        reporter.copy_cancellable(source, &mut writer, cancel)
    };

    if let Err(error) = result {
        drop(to);
        let _ = std::fs::remove_file(target);
        return Err(error);
    }

    reporter.finish();
    Ok((hex_string(&hasher.finalize()), reporter.done))
}

/// Writes each chunk to `to` and feeds the same bytes to the hasher. With `io::sink()` as `to` it only hashes.
struct HashingWriter<'a, D, W> {
    to: &'a mut W,
    hasher: &'a mut D,
}

impl<D: Digest, W: Write> Write for HashingWriter<'_, D, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.to.write(buf)?;
        Digest::update(self.hasher, &buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.to.flush()
    }
}

/// Percentage of work done, counted in bytes. It is reported only when it grows, so the receiver does not get
/// an event after every chunk.
pub struct Reporter<'a> {
    total: u64,
    done: u64,
    last: u32,
    report: &'a mut dyn FnMut(u32),
    /// Reused by every file of an extraction; a fresh 1 MiB allocation per file would dominate small mods.
    buffer: Vec<u8>,
}

impl<'a> Reporter<'a> {
    /// `total` is all the work in bytes. When it is 0, finishing reports 100% at once.
    pub fn new(total: u64, report: &'a mut dyn FnMut(u32)) -> Self {
        Self {
            total,
            done: 0,
            last: 0,
            report,
            buffer: vec![0; CHUNK],
        }
    }

    /// Copies `from` to `to` in chunks and adds each chunk to the progress. The buffer is owned by the reporter, so a
    /// loop over thousands of archive entries does not allocate one per entry.
    pub fn copy<R: Read + ?Sized>(&mut self, from: &mut R, to: &mut impl Write) -> io::Result<()> {
        self.copy_cancellable(from, to, None)
    }

    /// Copies like `copy`, stopping with `io::ErrorKind::Interrupted` at the next chunk when `cancel` is set.
    pub fn copy_cancellable<R: Read + ?Sized>(
        &mut self,
        from: &mut R,
        to: &mut impl Write,
        cancel: Option<&AtomicBool>,
    ) -> io::Result<()> {
        loop {
            if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }

            let read = from.read(&mut self.buffer)?;

            if read == 0 {
                return Ok(());
            }

            to.write_all(&self.buffer[..read])?;
            self.add(read as u64);
        }
    }

    /// Counts `bytes` more as done and reports the progress to the caller.
    pub fn add(&mut self, bytes: u64) {
        self.done = self.done.saturating_add(bytes);
        // A missing total means nothing to divide by, so the bar reads as done.
        let percent = crate::domain::progress::percent(self.done, self.total).unwrap_or(100);
        self.set(percent);
    }

    /// Ends the stage: afterwards the progress is always 100%, also for an empty file.
    pub fn finish(&mut self) {
        self.set(100);
    }

    fn set(&mut self, percent: u32) {
        if percent > self.last {
            self.last = percent;
            (self.report)(percent);
        }
    }
}

/// The hash of a file as hex, for example `Sha256` or `Sha512`. Reads the file in chunks, so large archives do not go
/// into memory, and reports the progress as a percentage. Used by the tool prerequisites and by a pack's archive check.
pub fn digest_file<D: Digest>(path: &Path, progress: &mut dyn FnMut(u32)) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = D::new();
    let mut reporter = Reporter::new(file.metadata()?.len(), progress);
    let mut sink = io::sink();
    reporter.copy(
        &mut file,
        &mut HashingWriter {
            to: &mut sink,
            hasher: &mut hasher,
        },
    )?;
    reporter.finish();
    Ok(hex_string(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use sha2::Sha256;

    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn sha256_matches_the_known_digest() {
        let dir = scratch!("sha");
        let file = dir.join("abc.txt");
        std::fs::write(&file, "abc").unwrap();
        assert_eq!(
            digest_file::<Sha256>(&file, &mut |_| {}).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_progress_grows_and_ends_at_100() {
        let dir = scratch!("sha-progress");
        let file = dir.join("big.bin");
        std::fs::write(&file, vec![7u8; 3 * CHUNK + 5]).unwrap();
        let mut seen = Vec::new();
        digest_file::<Sha256>(&file, &mut |percent| seen.push(percent)).unwrap();
        assert_eq!(seen.last(), Some(&100));
        assert!(seen.windows(2).all(|pair| pair[0] < pair[1]), "{seen:?}");
    }

    #[test]
    fn an_archive_is_copied_and_hashed_in_one_pass() {
        let dir = scratch!("archive");
        let source = dir.join("Mod 1.2.0.zip");
        std::fs::write(&source, "abc").unwrap();
        let working = dir.join("working");
        std::fs::create_dir_all(&working).unwrap();
        let copy = working.join("archive.zip");
        let digest = hash_and_copy::<Sha256>(&source, &copy, &mut |_| {}).unwrap();
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "abc");
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_failed_hash_and_copy_leaves_no_target() {
        let dir = scratch!("archive-fail");
        let source = dir.join("missing.zip");
        let target = dir.join("copy.zip");
        assert!(hash_and_copy::<Sha256>(&source, &target, &mut |_| {}).is_err());
        assert!(!target.exists());
    }

    #[test]
    fn a_reader_is_copied_and_hashed_with_the_bytes_counted() {
        let dir = scratch!("reader");
        let target = dir.join("copy.zip");
        let mut source: &[u8] = b"abc";
        let (digest, copied) =
            hash_and_copy_reader::<Sha256, _>(&mut source, &target, 3, &mut |_| {}).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "abc");
        assert_eq!(copied, 3);
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
