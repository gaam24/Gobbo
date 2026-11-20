//! Finding the helper binaries Gobbo ships beside its executable. The installed layout has them next to `gobbo.exe`;
//! the dev layout has them in the Cargo profile folder (`target/debug`). Cargo puts a package's test binary inside its
//! own build directory (`target/debug/build/<crate>/<hash>/out` with the build-dir layout, `target/debug/deps` before
//! it), so looking only one folder above the executable no longer reaches the dev layout. One bounded walk serves every
//! caller: the hook, the 32-bit helpers and the tests that drive them.

use std::path::{Path, PathBuf};

/// How many folders above the executable to look. The build-dir layout nests a test binary four levels below the
/// profile folder (`build/<crate>/<hash>/out`), so four covers it with the old `deps` layout as well. The walk stays
/// bounded so an installed copy cannot pick up a stray helper from an unrelated parent folder.
const MAX_PARENTS: usize = 4;

/// A helper binary next to the current executable, or in the dev layout above it. `None` when it was never built.
pub fn adjacent(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    find_from(exe.parent()?, name)
}

/// Looks for `name` in a folder and its parents, stopping after `MAX_PARENTS` steps. Kept separate from `adjacent` so
/// the search can be tested against a synthetic Cargo layout without depending on where the test runner lives.
fn find_from(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut dir = Some(dir);

    for _ in 0..=MAX_PARENTS {
        let candidate = dir?.join(name);

        if candidate.is_file() {
            return Some(candidate);
        }

        dir = dir?.parent();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{scratch, write};

    #[test]
    fn finds_a_helper_next_to_the_executable() {
        let base = scratch!("binaries-next-to-exe");
        write(&base.join("helper.dll"), "here");

        assert_eq!(
            find_from(&base, "helper.dll"),
            Some(base.join("helper.dll"))
        );
    }

    #[test]
    fn finds_a_helper_in_the_cargo_dev_layout_above_a_test_binary() {
        let base = scratch!("binaries-dev-layout");
        let out = base.join("build/gobbo/abc123/out");
        std::fs::create_dir_all(&out).unwrap();
        write(&base.join("helper.dll"), "here");
        write(&out.join("test.exe"), "exe");

        assert_eq!(find_from(&out, "helper.dll"), Some(base.join("helper.dll")));
    }

    #[test]
    fn does_not_walk_past_the_bounded_number_of_parents() {
        let base = scratch!("binaries-too-deep");
        let deep = base.join("a/b/c/d/e/f");
        std::fs::create_dir_all(&deep).unwrap();
        write(&base.join("helper.dll"), "here");

        assert_eq!(find_from(&deep, "helper.dll"), None);
    }
}
