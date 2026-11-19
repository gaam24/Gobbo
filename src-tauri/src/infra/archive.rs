//! Reads mod archives in the formats mods come in: zip, 7z and rar. Every format gives the same list of entries, and
//! the wanted entries are read in one pass in archive order, so the install code does not depend on the format.
//! 7z and rar have no random access, so each pass opens the file again and reads it from the start.

use std::borrow::Cow;
use std::collections::BTreeSet;

use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::error::{AppError, ErrorKind, bad_archive};

/// Whether a file name ends in an archive extension we read.
pub fn is_archive(file_name: &str) -> bool {
    crate::domain::archive::extension_of(file_name).is_some()
}

/// The archive formats we read. The format comes from the file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Zip,
    SevenZ,
    Rar,
}

impl Format {
    fn of(path: &Path) -> Option<Self> {
        match crate::domain::archive::extension_of(path.to_str()?)? {
            "zip" => Some(Self::Zip),
            "7z" => Some(Self::SevenZ),
            "rar" => Some(Self::Rar),
            _ => None,
        }
    }
}

/// The most entries an archive may list. The import writes at most 100,000 files (`import::install::MAX_EXTRACT_FILES`),
/// and an archive also lists folders and files the game layout skips, so this keeps a 2x margin. A list this large stays
/// at tens of MB for ordinary names and is refused before anything is extracted.
const MAX_ARCHIVE_ENTRIES: usize = 200_000;

/// The most bytes the entries of an archive may expand to. The import writes at most 16 GiB
/// (`import::install::MAX_EXTRACT_BYTES`), and this keeps a 2x margin for entries the layout skips. A normal mod pack is
/// far smaller; a larger header is a mistake or a decompression bomb.
const MAX_ARCHIVE_BYTES: u64 = 32 * 1024 * 1024 * 1024;

/// A problem with the archive: it is damaged, encrypted, uses a feature we do not read, or is too large to be a mod.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The archive is damaged, encrypted or uses a feature we do not read.
    #[error("{0}")]
    Detail(String),
    /// The archive lists more entries than a mod may have.
    #[error("the archive has {count} entries; the limit is {limit}")]
    TooManyEntries { count: usize, limit: usize },
    /// The entries of the archive expand to more bytes than a mod may have.
    #[error("the archive expands to {bytes} bytes; the limit is {limit}")]
    TooLarge { bytes: u64, limit: u64 },
}

impl Error {
    fn new(detail: impl fmt::Display) -> Self {
        Self::Detail(detail.to_string())
    }
}

impl From<Error> for AppError {
    fn from(error: Error) -> Self {
        match error {
            Error::Detail(detail) => bad_archive(detail),
            Error::TooManyEntries { count, limit } => AppError::new("archive.too_many_entries")
                .with("count", count)
                .with("limit", limit)
                .kind(ErrorKind::Permanent),
            Error::TooLarge { bytes, limit } => AppError::new("archive.too_large")
                .with("bytes", bytes)
                .with("limit", limit)
                .kind(ErrorKind::Permanent),
        }
    }
}

/// Checks the entry count and the summed uncompressed size of `entries` against `count_limit` and `byte_limit`. The sum
/// saturates, so a header that declares very large sizes cannot wrap into a small total. The limits are parameters so a
/// test can use a small synthetic list instead of a 200,000-entry archive.
fn limits_ok(entries: &[Entry], count_limit: usize, byte_limit: u64) -> Result<(), Error> {
    let count = entries.len();
    if count > count_limit {
        return Err(Error::TooManyEntries {
            count,
            limit: count_limit,
        });
    }
    let bytes = entries
        .iter()
        .fold(0u64, |total, entry| total.saturating_add(entry.size));
    if bytes > byte_limit {
        return Err(Error::TooLarge {
            bytes,
            limit: byte_limit,
        });
    }
    Ok(())
}

/// One file or folder in the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The path in the archive with `/` as the separator, for example `Mods/modA/content/x.ws`.
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// An archive with its list of entries. The data is read with `visit` or `read_bytes`.
#[derive(Debug)]
pub struct Archive {
    path: PathBuf,
    format: Format,
    entries: Vec<Entry>,
}

impl Archive {
    /// Lists the entries of the archive at `path`. The format comes from the extension.
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::open_limited(path, MAX_ARCHIVE_ENTRIES, MAX_ARCHIVE_BYTES)
    }

    /// Lists `path` and applies the archive-wide limits. `open` uses the production limits; a test passes small ones to
    /// reach the check with a small archive.
    fn open_limited(path: &Path, count_limit: usize, byte_limit: u64) -> Result<Self, Error> {
        let format = Format::of(path)
            .ok_or_else(|| Error::new(format!("{} is not a supported archive", path.display())))?;
        let entries = match format {
            Format::Zip => list_zip(path)?,
            Format::SevenZ => list_7z(path)?,
            Format::Rar => list_rar(path)?,
        };
        limits_ok(&entries, count_limit, byte_limit)?;
        Ok(Self {
            path: path.to_path_buf(),
            format,
            entries,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Reads the entries with the given indexes (positions in `entries()`), in archive order. `visit` gets each
    /// entry's index and its data, which is valid only during the call.
    pub fn visit<E: From<Error>>(
        &self,
        wanted: &BTreeSet<usize>,
        mut visit: impl FnMut(usize, &mut dyn Read) -> Result<(), E>,
    ) -> Result<(), E> {
        match self.format {
            Format::Zip => visit_zip(&self.path, wanted, &mut visit),
            Format::SevenZ => visit_7z(&self.path, &self.entries, wanted, &mut visit),
            Format::Rar => visit_rar(&self.path, wanted, &mut visit),
        }
    }

    /// The content of one entry, refusing an entry larger than `limit`. The reader stops at `limit + 1` bytes, so a
    /// malicious archive cannot make the caller allocate an unbounded buffer.
    pub fn read_bytes_limited(&self, index: usize, limit: u64) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::new();
        self.visit::<Error>(&BTreeSet::from([index]), |_, data| {
            data.take(limit.saturating_add(1))
                .read_to_end(&mut bytes)
                .map_err(Error::new)?;
            if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
                return Err(Error::new("archive entry is too large"));
            }
            Ok(())
        })?;
        Ok(bytes)
    }
}

fn open(path: &Path) -> Result<File, Error> {
    File::open(path).map_err(|e| Error::new(format!("{}: {e}", path.display())))
}

fn entry_name(name: &str) -> Cow<'_, str> {
    if name.contains('\\') {
        Cow::Owned(name.replace('\\', "/"))
    } else {
        Cow::Borrowed(name)
    }
}

fn list_zip(path: &Path) -> Result<Vec<Entry>, Error> {
    let mut archive = zip::ZipArchive::new(open(path)?).map_err(Error::new)?;
    (0..archive.len())
        .map(|index| {
            let entry = archive.by_index(index).map_err(Error::new)?;
            Ok(Entry {
                name: entry_name(&entry.name().map_err(Error::new)?).into_owned(),
                is_dir: entry.is_dir(),
                size: entry.size(),
            })
        })
        .collect()
}

/// What a visit does with one wanted entry: it gets the entry's index and content, and fails with `E`.
type Visitor<'a, E> = dyn FnMut(usize, &mut dyn Read) -> Result<(), E> + 'a;

fn visit_zip<E: From<Error>>(
    path: &Path,
    wanted: &BTreeSet<usize>,
    visit: &mut Visitor<'_, E>,
) -> Result<(), E> {
    let mut archive = zip::ZipArchive::new(open(path)?).map_err(Error::new)?;
    for &index in wanted {
        let mut entry = archive.by_index(index).map_err(Error::new)?;
        visit(index, &mut entry)?;
    }
    Ok(())
}

fn open_7z(path: &Path) -> Result<sevenz_rust2::ArchiveReader<File>, Error> {
    let file = open(path)?;
    sevenz_rust2::ArchiveReader::new(file, sevenz_rust2::Password::empty()).map_err(Error::new)
}

fn list_7z(path: &Path) -> Result<Vec<Entry>, Error> {
    let reader = open_7z(path)?;
    let entries: Vec<Entry> = reader
        .archive()
        .files
        .iter()
        .map(|file| Entry {
            name: entry_name(file.name()).into_owned(),
            is_dir: file.is_directory(),
            size: file.size(),
        })
        .collect();
    // Entries are matched to the list by name when read, so two entries with one name cannot be told apart.
    let mut seen = FxHashSet::default();
    if let Some(entry) = entries
        .iter()
        .find(|entry| !seen.insert(entry.name.as_str()))
    {
        return Err(Error::new(format!("two entries are named {}", entry.name)));
    }
    Ok(entries)
}

fn visit_7z<E: From<Error>>(
    path: &Path,
    entries: &[Entry],
    wanted: &BTreeSet<usize>,
    visit: &mut Visitor<'_, E>,
) -> Result<(), E> {
    let index_of: FxHashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.name.as_str(), index))
        .collect();
    let mut reader = open_7z(path)?;
    let mut failure = None;
    // The library decodes the entries of a solid block in order, so every entry must be read to the end, even the
    // ones we skip. The callback stops early only when `visit` fails.
    let result = reader.for_each_entries(|file, data| {
        let name = entry_name(file.name());
        let index = index_of.get(name.as_ref()).copied();
        if let Some(index) = index.filter(|index| wanted.contains(index))
            && let Err(error) = visit(index, data)
        {
            failure = Some(error);
            return Ok(false);
        }
        io::copy(data, &mut io::sink())?;
        Ok(true)
    });
    if let Some(error) = failure {
        return Err(error);
    }
    result.map_err(Error::new)?;
    Ok(())
}

fn list_rar(path: &Path) -> Result<Vec<Entry>, Error> {
    unrar::Archive::new(path)
        .open_for_listing()
        .map_err(Error::new)?
        .map(|header| {
            let header = header.map_err(Error::new)?;
            Ok(Entry {
                name: entry_name(&header.filename.to_string_lossy()).into_owned(),
                is_dir: header.is_directory(),
                size: header.unpacked_size,
            })
        })
        .collect()
}

fn visit_rar<E: From<Error>>(
    path: &Path,
    wanted: &BTreeSet<usize>,
    visit: &mut Visitor<'_, E>,
) -> Result<(), E> {
    let mut cursor = unrar::Archive::new(path)
        .open_for_processing()
        .map_err(Error::new)?;
    // The library reads only into a file, so a wanted entry goes to a scratch folder first and is read from there. The
    // folder and file are reused for every entry; creating a TempDir per entry was a large part of a RAR import.
    let scratch = Scratch::new().map_err(Error::new)?;
    let file = scratch.path().join("entry");
    let mut index = 0;
    while let Some(header) = cursor.read_header().map_err(Error::new)? {
        cursor = if wanted.contains(&index) {
            let _ = std::fs::remove_file(&file);
            let next = header.extract_to(&file).map_err(Error::new)?;
            visit(index, &mut open(&file)?)?;
            next
        } else {
            header.skip().map_err(Error::new)?
        };
        index += 1;
    }
    Ok(())
}

/// A folder in the temp directory for one entry of a rar archive. `tempfile` creates it with a unique name and removes
/// it on drop, including when the extraction fails.
struct Scratch(tempfile::TempDir);

impl Scratch {
    fn new() -> io::Result<Self> {
        tempfile::TempDir::new().map(Self)
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::sync::OnceLock;

    use proptest::prelude::*;

    use super::*;
    use crate::test_support::scratch;

    /// A rar archive with one entry, `VERSION`, copied from the `unrar` crate's test data.
    const RAR: &[u8] = include_bytes!("../../tests/fixtures/version.rar");

    fn temp_dir(name: &str) -> PathBuf {
        scratch!(name)
    }

    fn read_all(archive: &Archive) -> Vec<(String, String)> {
        let wanted: BTreeSet<usize> = (0..archive.entries().len()).collect();
        let mut out = Vec::new();
        archive
            .visit::<Error>(&wanted, |index, data| {
                let mut text = String::new();
                data.read_to_string(&mut text).map_err(Error::new)?;
                out.push((archive.entries()[index].name.clone(), text));
                Ok(())
            })
            .unwrap();
        out
    }

    fn file_entry(name: &str, size: u64) -> Entry {
        Entry {
            name: name.to_string(),
            is_dir: false,
            size,
        }
    }

    #[test]
    fn strips_only_supported_extensions() {
        assert_eq!(
            crate::domain::archive::strip_extension("Mod 1.2.zip"),
            "Mod 1.2"
        );
        assert_eq!(
            crate::domain::archive::strip_extension("Mod 1.2.7Z"),
            "Mod 1.2"
        );
        assert_eq!(crate::domain::archive::strip_extension("Mod.rar"), "Mod");
        assert_eq!(
            crate::domain::archive::strip_extension("Mod.tar"),
            "Mod.tar"
        );
        assert_eq!(crate::domain::archive::strip_extension("Mod"), "Mod");
    }

    #[test]
    fn reads_zip_entries() {
        let dir = temp_dir("zip");
        let path = dir.join("mod.zip");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        writer
            .start_file(
                "Mods/modA/content/x.ws",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"zip text").unwrap();
        writer.finish().unwrap();

        let archive = Archive::open(&path).unwrap();
        assert_eq!(
            archive.entries(),
            [Entry {
                name: "Mods/modA/content/x.ws".to_string(),
                is_dir: false,
                size: 8,
            }]
        );
        assert_eq!(
            read_all(&archive),
            [("Mods/modA/content/x.ws".to_string(), "zip text".to_string())]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_an_archive_applies_both_limits() {
        let dir = temp_dir("limits-open");
        let path = dir.join("mod.zip");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        writer
            .start_file("x.ws", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"small").unwrap();
        writer.finish().unwrap();

        match Archive::open_limited(&path, 0, u64::MAX).unwrap_err() {
            Error::TooManyEntries { count, limit } => {
                assert_eq!(count, 1);
                assert_eq!(limit, 0);
            }
            other => panic!("unexpected error: {other}"),
        }
        match Archive::open_limited(&path, usize::MAX, 4).unwrap_err() {
            Error::TooLarge { bytes, limit } => {
                assert_eq!(bytes, 5);
                assert_eq!(limit, 4);
            }
            other => panic!("unexpected error: {other}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_archive_over_the_byte_limit_is_refused() {
        let entries = [file_entry("a.ws", 10), file_entry("b.ws", 11)];
        match limits_ok(&entries, usize::MAX, 20).unwrap_err() {
            Error::TooLarge { bytes, limit } => {
                assert_eq!(bytes, 21);
                assert_eq!(limit, 20);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn a_size_sum_that_overflows_saturates() {
        let entries = [file_entry("a.ws", u64::MAX), file_entry("b.ws", 1)];
        match limits_ok(&entries, usize::MAX, u64::MAX - 1).unwrap_err() {
            Error::TooLarge { bytes, .. } => assert_eq!(bytes, u64::MAX),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn a_limit_error_carries_its_code_and_values() {
        let entries = [file_entry("a.ws", 0), file_entry("b.ws", 0)];
        let error: AppError = limits_ok(&entries, 1, u64::MAX).unwrap_err().into();
        assert_eq!(error.code, "archive.too_many_entries");
        assert_eq!(error.params["count"], "2");
        assert_eq!(error.params["limit"], "1");

        let error: AppError = limits_ok(&[file_entry("a.ws", 2)], usize::MAX, 1)
            .unwrap_err()
            .into();
        assert_eq!(error.code, "archive.too_large");
        assert_eq!(error.params["bytes"], "2");
        assert_eq!(error.params["limit"], "1");
    }

    #[test]
    fn reads_every_zip_compression_method() {
        use zip::CompressionMethod;

        let methods = [
            CompressionMethod::Stored,
            CompressionMethod::Deflated,
            CompressionMethod::Bzip2,
            CompressionMethod::Zstd,
            CompressionMethod::Xz,
            CompressionMethod::Ppmd,
        ];
        for (at, method) in methods.into_iter().enumerate() {
            let dir = temp_dir(&format!("zip-method-{at}"));
            let path = dir.join("mod.zip");
            let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
            writer
                .start_file(
                    "x.ws",
                    zip::write::SimpleFileOptions::default().compression_method(method),
                )
                .unwrap();
            writer.write_all(b"codec text").unwrap();
            writer.finish().unwrap();

            let archive = Archive::open(&path).unwrap();
            assert_eq!(
                read_all(&archive),
                [("x.ws".to_string(), "codec text".to_string())],
                "{method:?}"
            );
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn reads_7z_entries() {
        let dir = temp_dir("7z");
        let source = dir.join("x.ws");
        fs::write(&source, "7z text").unwrap();
        let path = dir.join("mod.7z");
        let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
        let folder = sevenz_rust2::ArchiveEntry::from_path(&dir, "Mods/modA".to_string());
        writer.push_archive_entry::<&[u8]>(folder, None).unwrap();
        let file =
            sevenz_rust2::ArchiveEntry::from_path(&source, "Mods/modA/content/x.ws".to_string());
        writer
            .push_archive_entry(file, Some(File::open(&source).unwrap()))
            .unwrap();
        writer.finish().unwrap();

        let archive = Archive::open(&path).unwrap();
        let names: Vec<(&str, bool)> = archive
            .entries()
            .iter()
            .map(|e| (e.name.as_str(), e.is_dir))
            .collect();
        assert!(names.contains(&("Mods/modA/content/x.ws", false)));
        assert!(names.contains(&("Mods/modA", true)));
        assert!(
            read_all(&archive)
                .contains(&("Mods/modA/content/x.ws".to_string(), "7z text".to_string()))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_rar_entries() {
        let dir = temp_dir("rar");
        let path = dir.join("mod.rar");
        fs::write(&path, RAR).unwrap();

        let archive = Archive::open(&path).unwrap();
        let names: Vec<&str> = archive.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["VERSION"]);
        assert_eq!(read_all(&archive).len(), 1);
        assert_eq!(
            archive.read_bytes_limited(0, u64::MAX).unwrap(),
            read_all(&archive)[0].1.as_bytes()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_other_extensions() {
        let error = Archive::open(Path::new("mod.tar")).unwrap_err();
        assert!(error.to_string().contains("not a supported archive"));
    }

    /// One folder for every case of the property test. The three files are overwritten in place, so a 64-case run does
    /// not create 192 folders.
    fn fuzz_dir() -> &'static Path {
        static DIR: OnceLock<PathBuf> = OnceLock::new();
        DIR.get_or_init(|| temp_dir("fuzz"))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Bytes that claim to be an archive are a typed error or a listing, never a panic or a hang. Opening lists the
        /// entries only; it does not decompress them.
        #[test]
        fn arbitrary_bytes_are_listed_or_refused(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
            for extension in ["zip", "7z", "rar"] {
                let path = fuzz_dir().join(format!("mod.{extension}"));
                fs::write(&path, &bytes).unwrap();
                match Archive::open(&path) {
                    Ok(archive) => {
                        prop_assert!(archive.entries().len() <= MAX_ARCHIVE_ENTRIES);
                    }
                    Err(error) => {
                        prop_assert!(!error.to_string().is_empty());
                    }
                }
            }
        }
    }
}
