//! The problems a check collects, the section each one belongs to, and the small rules that say whether one value is
//! well formed.

use std::collections::BTreeSet;
use std::fmt;

use super::super::definition::Requirement;
use super::super::layout::is_game_relative;
use crate::domain::deploy::Deploy;
use crate::domain::pathkey;

/// The part of a definition a check found a problem in. This is set where the message is written, so rewording a
/// message cannot move it to another editor section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::games) enum Section {
    /// Schema, id, name and providers: the editor's general section.
    General,
    /// A definition-level problem without a more specific field; also the merged loader package.
    Source,
    /// A loader reference checked on the game, or a rule of a loader preset.
    Loader,
    /// Markers, executables, required files and requirements.
    Detection,
    /// User-data sets and the profile data templates.
    UserData,
    /// Slots, mirrors, aliases, roots and root files.
    Deploy,
    /// The load-order file and priority range.
    LoadOrder,
    /// Install steps.
    Steps,
    /// Tools and their prerequisites.
    Tools,
    /// Log sources.
    Logs,
    /// The merged loader package's own fields, shown in the source section today.
    Install,
}

/// One problem and the section of the definition that owns it. Only the message is shown to the developer.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::games) struct Problem {
    pub(in crate::games) section: Section,
    pub(in crate::games) message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// The problems found so far. Each check adds its own; the list is the result of `validate`.
#[derive(Default)]
pub(in crate::games) struct Problems(pub(in crate::games) Vec<Problem>);

impl Problems {
    pub(super) fn push_in(&mut self, section: Section, message: impl Into<String>) {
        self.0.push(Problem {
            section,
            message: message.into(),
        });
    }

    /// `min_gobbo`: a release number such as `0.3.0`, no newer than this app. A newer floor refuses the file, so an
    /// old app never serves a definition that depends on a later fix.
    pub(super) fn min_gobbo(&mut self, section: Section, what: &str, min: &str) {
        let app = env!("CARGO_PKG_VERSION");

        if !is_release_version(min) {
            self.push_in(
                section,
                format!("{what} min_gobbo must be a release number such as 0.3.0: {min:?}"),
            );
        } else if crate::domain::version::compare(min, app) == std::cmp::Ordering::Greater {
            self.push_in(
                section,
                format!("{what} needs Gobbo {min} or newer; this is {app}"),
            );
        }
    }

    /// Every path must be relative and stay inside the game (or the mod): no drive, no leading `/`, no `..`.
    pub(super) fn relative<'a>(
        &mut self,
        section: Section,
        what: &str,
        paths: impl IntoIterator<Item = &'a String>,
    ) {
        for path in paths {
            if !is_game_relative(path) {
                self.push_in(
                    section,
                    format!("{what} is not a relative path inside the game: {path:?}"),
                );
            }
        }
    }

    /// A path template: not empty and never leaving its folder through a `..` segment. A name that only contains two
    /// dots, such as `Team..Saves`, is an ordinary folder name.
    pub(super) fn template(&mut self, section: Section, what: &str, template: &str) {
        if template.trim().is_empty() || template.split(['/', '\\']).any(|part| part == "..") {
            self.push_in(
                section,
                format!("{what} is not a safe path template: {template:?}"),
            );
        }
    }

    /// Requirement metadata: a name, a relative path and, when present, the HTTPS page that offers the file.
    pub(super) fn requirements(&mut self, section: Section, what: &str, list: &[Requirement]) {
        for requirement in list {
            if requirement.name.trim().is_empty() {
                self.push_in(section, format!("{what} requirement name is empty"));
            }

            if !is_game_relative(&requirement.path) {
                self.push_in(
                    section,
                    format!(
                        "{what} requirement path is not relative inside the game: {:?}",
                        requirement.path
                    ),
                );
            }

            if let Some(url) = &requirement.url
                && !url.starts_with("https://")
            {
                self.push_in(
                    section,
                    format!("{what} requirement URL is not HTTPS: {url:?}"),
                );
            }
        }
    }

    /// Extensions as the code compares them: lowercase, with the dot.
    pub(super) fn extensions<'a>(
        &mut self,
        section: Section,
        what: &str,
        extensions: impl IntoIterator<Item = &'a String>,
    ) {
        for extension in extensions {
            let well_formed = extension.len() > 1
                && extension.starts_with('.')
                && *extension == extension.to_lowercase();
            if !well_formed {
                self.push_in(
                    section,
                    format!(
                        "{what}: extension must be lowercase and start with a dot: {extension:?}"
                    ),
                );
            }
        }
    }
}

/// The three duplicate rules a merge would hide, shared by the game and the loader checks. The rule and the message
/// are the same on both sides; the caller passes the section that owns them in its own definition.
#[derive(Default)]
pub(in crate::games) struct Duplicates {
    folders: BTreeSet<String>,
    aliases: BTreeSet<String>,
    root_files: BTreeSet<String>,
}

impl Duplicates {
    pub(in crate::games) fn folder(
        &mut self,
        dir: &str,
        section: Section,
        problems: &mut Problems,
    ) {
        if !self.folders.insert(dir.to_lowercase()) {
            problems.push_in(section, format!("folder {dir:?} is a slot or mirror twice"));
        }
    }

    pub(in crate::games) fn alias(
        &mut self,
        name: &str,
        section: Section,
        problems: &mut Problems,
    ) {
        if !self.aliases.insert(pathkey::path_key(name)) {
            problems.push_in(
                section,
                format!("alias {name:?} is repeated (case is ignored)"),
            );
        }
    }

    pub(in crate::games) fn root_file(
        &mut self,
        name: &str,
        section: Section,
        problems: &mut Problems,
    ) {
        if !self.root_files.insert(pathkey::path_key(name)) {
            problems.push_in(section, format!("root file {name:?} is listed twice"));
        }
    }
}

/// The duplicate problems of one deploy table, checked before its layers are merged.
pub(in crate::games) fn duplicate_problems(deploy: &Deploy, section: Section) -> Vec<Problem> {
    let mut problems = Problems::default();
    let mut duplicates = Duplicates::default();

    for slot in &deploy.slots {
        duplicates.folder(&slot.dir, section, &mut problems);
    }

    for mirror in &deploy.mirror {
        duplicates.folder(&mirror.dir, section, &mut problems);
    }

    for name in deploy.aliases.keys() {
        duplicates.alias(name, section, &mut problems);
    }

    for name in &deploy.root_files {
        duplicates.root_file(name, section, &mut problems);
    }

    problems.0
}

/// A loader variant: lowercase letters, digits, `-`, `_` or `.`, with no path and no relative segment.
pub(in crate::games) fn is_variant(text: &str) -> bool {
    !text.is_empty()
        && text != "."
        && text != ".."
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

/// What a provider needs to address a game: letters, digits, `-`, `_` or `.`, with no path and no relative segment.
pub(super) fn is_provider_key(text: &str) -> bool {
    !text.is_empty()
        && text != "."
        && text != ".."
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// 128 lowercase hex digits: a SHA-512 as `sha2` writes it.
pub(super) fn is_sha512(text: &str) -> bool {
    text.len() == 128
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A release number: three dot-separated groups of digits, such as `0.3.0`.
pub(in crate::games) fn is_release_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}
