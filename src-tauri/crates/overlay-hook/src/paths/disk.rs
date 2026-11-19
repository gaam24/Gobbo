//! The plain file-system checks the hook makes itself. They are not hooked, so they never recurse into the hook.

use std::fs;
use std::path::Path;

use super::probe::probe;

/// Whether a path exists. One attribute query, where `Path::exists` opens the file to ask. A dangling symbolic link
/// exists as the link itself, which is how Windows lists it.
pub(crate) fn exists(path: &Path) -> bool {
    probe(path).is_found()
}

/// Creates the folder that will hold `path`, when it is missing. The caller decides how to report a failure. A folder
/// that is already there costs one attribute query; `create_dir_all` would ask twice on every write.
pub(crate) fn make_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !exists(parent)
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dangling_symbolic_link_exists_as_the_link_itself() {
        let base = std::env::temp_dir().join(format!("gobbo-link-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        let link = base.join("link.txt");
        // Creating a symbolic link needs developer mode or the privilege; without it the check is skipped.
        if std::os::windows::fs::symlink_file(base.join("missing.txt"), &link).is_ok() {
            assert!(exists(&link));
        }
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_long_path_exists_as_it_does_for_the_standard_library() {
        let base = std::env::temp_dir().join(format!("gobbo-exists-{}", std::process::id()));
        let long = base
            .join("x".repeat(100))
            .join("y".repeat(100))
            .join("z".repeat(100));
        // The standard library takes the long path itself, so the folder and its file are made.
        fs::create_dir_all(&long).unwrap();
        let file = long.join("file.txt");
        fs::write(&file, "x").unwrap();

        assert!(file.as_os_str().len() > 260);
        assert!(exists(&file));
        assert!(!exists(&long.join("missing.txt")));
        assert!(exists(&base));
        fs::remove_dir_all(&base).unwrap();
    }
}
