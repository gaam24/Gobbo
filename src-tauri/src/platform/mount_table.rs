//! The text of a mount table (`/proc/<pid>/mountinfo`): whether an overlay is mounted on a folder. The text is parsed the
//! same way on every platform, so these rules are tested everywhere; only the reader of the table is Linux-only.

use std::path::Path;

/// Whether a mount table has an overlay on `root`. The mount point is the fifth field of a line, with its spaces
/// written as octal escapes, and the file system type comes right after the ` - ` separator.
pub fn mounted_overlay_at(table: &str, root: &Path) -> bool {
    let root = root.to_string_lossy();
    let root = root.trim_end_matches('/');
    table.lines().any(|line| {
        let Some((before, after)) = line.split_once(" - ") else {
            return false;
        };
        let fs_type = after.split(' ').next();
        let point = before.split(' ').nth(4).map(unescape_mount);
        fs_type == Some("overlay") && point.as_deref() == Some(root)
    })
}

/// Undoes the octal escapes of a mount table: `\040` is a space, `\134` is a backslash.
fn unescape_mount(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('\\') {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        let octal = rest
            .get(1..4)
            .filter(|digits| digits.bytes().all(|byte| (b'0'..=b'7').contains(&byte)));
        if let Some(byte) = octal.and_then(|digits| u8::from_str_radix(digits, 8).ok()) {
            out.push(char::from(byte));
            rest = &rest[4..];
        } else {
            out.push('\\');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mount table with an overlay on a game folder whose name has a space. The space is written as `\040`. The second
    /// line is the folder above it, on another file system.
    const TABLE: &str = r"1100 686 0:105 / /data/library/Example\040Game rw,relatime - overlay overlay rw,lowerdir=/a:/b,upperdir=/c,workdir=/d
1101 686 0:44 / /data/library rw,relatime - ext4 /dev/example rw";

    const GAME: &str = "/data/library/Example Game";

    #[test]
    fn an_overlay_on_the_game_folder_is_found_through_its_escaped_name() {
        assert!(mounted_overlay_at(TABLE, Path::new(GAME)));
    }

    #[test]
    fn a_trailing_slash_names_the_same_folder() {
        assert!(mounted_overlay_at(TABLE, Path::new(&format!("{GAME}/"))));
    }

    #[test]
    fn another_folder_or_another_file_system_is_not_the_overlay() {
        assert!(!mounted_overlay_at(TABLE, Path::new("/data/library")));
        assert!(!mounted_overlay_at("", Path::new(GAME)));
    }

    #[test]
    fn octal_escapes_are_read_back() {
        assert_eq!(unescape_mount(r"a\040b\134c"), "a b\\c");
        assert_eq!(unescape_mount(r"no\escape\04x"), r"no\escape\04x");
    }
}
