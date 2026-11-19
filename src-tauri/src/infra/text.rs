//! Text files read from the end: the log viewer and the last launch output share one tail reader.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

/// The last `count` lines of a text file.
///
/// It reads from the end, so a long log costs only the bytes of its tail. Invalid UTF-8 is replaced, as the log shows
/// it. The first line of a read that stops inside the file may be cut, so it is dropped.
pub fn tail_lines(path: &std::path::Path, count: usize) -> io::Result<Vec<String>> {
    tail_lines_with_limit(path, count, u64::MAX)
}

/// `tail_lines` with a byte limit: a log without newlines cannot make the reader walk the whole file. The limit is a
/// ceiling, not an exact size; the last chunk can cross it by at most one `CHUNK`.
pub fn tail_lines_with_limit(
    path: &std::path::Path,
    count: usize,
    max_bytes: u64,
) -> io::Result<Vec<String>> {
    const CHUNK: u64 = 8 * 1024;
    let mut file = File::open(path)?;
    let mut end = file.metadata()?.len();
    // The chunks are read from the end, so they are joined once at the end: joining each chunk to the tail as it is read
    // would copy the whole tail again for every chunk.
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    let mut newlines = 0;
    let mut read = 0_u64;
    while end > 0 && newlines <= count && read < max_bytes {
        let size = CHUNK.min(max_bytes.saturating_sub(read));
        let start = end.saturating_sub(size);
        let mut bytes = vec![0; usize::try_from(end - start).unwrap_or_default()];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut bytes)?;
        newlines += memchr::memchr_iter(b'\n', &bytes).count();
        chunks.push(bytes);
        read += end - start;
        end = start;
    }
    chunks.reverse();
    let tail = chunks.concat();
    let text = String::from_utf8_lossy(&tail);
    let mut lines: Vec<&str> = text.lines().collect();
    if end > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let start = lines.len().saturating_sub(count);
    Ok(lines[start..]
        .iter()
        .map(|line| (*line).to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn tail_lines_reads_only_the_end_of_a_long_file() {
        let dir = scratch!("tail");
        let path = dir.join("log.txt");
        let body = (0..5_000)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(&path, body).unwrap();
        let tail = tail_lines(&path, 3).unwrap();
        assert_eq!(tail, ["line 4997", "line 4998", "line 4999"]);
    }

    #[test]
    fn tail_lines_of_a_short_file_are_all_its_lines() {
        let dir = scratch!("tail_short");
        let path = dir.join("log.txt");
        std::fs::write(&path, "one\r\ntwo\n").unwrap();
        assert_eq!(tail_lines(&path, 12).unwrap(), ["one", "two"]);
        std::fs::write(&path, "").unwrap();
        assert!(tail_lines(&path, 12).unwrap().is_empty());
    }
}
