//! Loader package rules: a name and version for the interface, a check path, an HTTPS source with a SHA-256, and paths
//! that stay inside the deployment root.

use super::problems::Problems;
use crate::games::definition::Install;

impl crate::games::definition::GameDef {
    /// The merged package must be complete; each layer's paths, link and hash were checked where they were read.
    pub(super) fn check_installs(&self, problems: &mut Problems) {
        let Some(install) = &self.install else {
            return;
        };
        let label = "install";
        check_install(label, install, problems);

        for (field, value) in [
            ("name", &install.name),
            ("version", &install.version),
            ("url", &install.url),
        ] {
            if value.trim().is_empty() {
                problems.push(format!("{label}: {field} is empty"));
            }
        }

        if install.check.is_empty() {
            problems.push(format!(
                "{label}: needs at least one check path that proves the loader is there"
            ));
        }
    }
}

/// The rules that hold for every layer of a package: an HTTPS link with a SHA-256, and paths that stay inside the
/// deployment root and are valid on both systems.
pub(in crate::games) fn check_install(label: &str, install: &Install, problems: &mut Problems) {
    problems.relative(&format!("{label} check path"), &install.check);
    problems.relative(&format!("{label} needed_by folder"), &install.needed_by);

    if !install.to.trim().is_empty() {
        problems.relative(&format!("{label} folder"), std::iter::once(&install.to));
    }

    for path in install
        .check
        .iter()
        .chain(&install.needed_by)
        .chain(std::iter::once(&install.to))
    {
        if !path_characters_ok(path) {
            problems.push(format!("{label}: path has an invalid name: {path:?}"));
        }
    }

    if !install.url.trim().is_empty() {
        if !install.url.starts_with("https://") {
            problems.push(format!("{label}: url must be https: {:?}", install.url));
        }

        if !is_sha256(&install.sha256) {
            problems.push(format!(
                "{label}: sha256 must be 64 hex digits: {:?}",
                install.sha256
            ));
        }
    }
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A loader path must be writable on both systems, not only the one that reads the definition now.
fn path_characters_ok(path: &str) -> bool {
    path.split(['/', '\\']).all(|part| {
        !part
            .chars()
            .any(crate::domain::naming::is_invalid_name_char)
    })
}

#[cfg(test)]
mod tests {
    use crate::games::definition::GameDef;
    use crate::test_support::games::SLOT;

    fn problems(source: &str) -> Vec<String> {
        let text = SLOT.replacen(
            "[deploy]",
            &format!(
                "[install]\nname = \"Loader\"\nversion = \"1.0\"\nto = \"bin\"\ncheck = [\"bin/proxy.dll\"]\n{source}\n\n[deploy]"
            ),
            1,
        );
        GameDef::parse(&text).unwrap().validate()
    }

    fn source(url: &str, sha256: &str) -> String {
        format!("url = \"{url}\"\nsha256 = \"{sha256}\"")
    }

    #[test]
    fn a_loader_package_needs_https_a_hash_and_safe_paths() {
        let hash = "a".repeat(64);
        let valid = problems(&source("https://example.com/loader.zip", &hash));
        assert!(valid.is_empty(), "{valid:?}");

        let bad_url = problems(&source("http://example.com/loader.zip", &hash));
        assert!(
            bad_url
                .iter()
                .any(|problem| problem.contains("must be https"))
        );

        let bad_hash = problems(&source("https://example.com/loader.zip", "abc"));
        assert!(bad_hash.iter().any(|problem| problem.contains("sha256")));

        let missing = problems("");
        assert!(
            missing
                .iter()
                .any(|problem| problem.contains("url is empty"))
        );
    }

    #[test]
    fn a_merged_package_needs_a_version_and_a_check_path() {
        let text = SLOT.replacen("[deploy]", "[install]\nname = \"Loader\"\n\n[deploy]", 1);
        let problems = GameDef::parse(&text).unwrap().validate();
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("version is empty"))
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("check path"))
        );
    }
}
