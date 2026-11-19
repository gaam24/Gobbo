//! The plan of one import: the game's rules, the wrapper and aliases, and the rule order that decides where each entry
//! goes.

use crate::games::{Deploy, Mirror, Slot};

use crate::import::install::Placed;
use crate::import::install::errors::InstallError;

use super::classify::{extension_allowed, mirror_path, starts_with_path};
use super::units::{Unit, UnitIndex};

pub(in crate::import::install) fn attach_mirrored(placed: &mut [Placed]) {
    let main = placed
        .iter()
        .filter(|item| !item.slot.is_empty())
        .map(|item| item.group.clone())
        .min();
    if let Some(main) = main {
        for item in placed.iter_mut().filter(|item| item.slot.is_empty()) {
            item.group.clone_from(&main);
        }
    }
}

/// The rules of one game's deployment, split into path parts once. An archive can have 20k entries, and splitting the
/// same slot and mirror folders for every entry was a large part of the layout pass.
pub(in crate::import::install) struct LayoutPlan<'a> {
    /// The definition's deploy rules, for the root files by name.
    deploy: &'a Deploy,
    pub(super) slots: Vec<SlotRule<'a>>,
    pub(super) mirrors: Vec<MirrorRule<'a>>,
    /// The mirrors by the first component of their folder, in the definition's order. An entry that starts with
    /// another folder (most of an archive) does not compare with every mirror.
    mirrors_by_first: Vec<(&'a str, Vec<usize>)>,
    pub(super) protected: Vec<Vec<&'a str>>,
    /// The archive folder aliases of the definition, by the folder name an archive uses, with the game folder's parts.
    aliases: Vec<(&'a str, Vec<&'a str>)>,
    /// The first component of every actual deploy place (slot, mirror, protected), for `wrapper_depth`.
    place_folders: Vec<&'a str>,
    /// The first component of every deploy place and alias name, for `wrapper_depth`.
    pub(super) game_folders: Vec<&'a str>,
}

pub(super) struct SlotRule<'a> {
    pub(super) slot: &'a Slot,
    pub(super) dir: Vec<&'a str>,
}

pub(super) struct MirrorRule<'a> {
    pub(super) mirror: &'a Mirror,
    pub(super) dir: Vec<&'a str>,
}

impl<'a> LayoutPlan<'a> {
    /// Builds the rules. `base_roots` are the top-level folders of the game itself, for example `bin`, `r6` and
    /// `archive`; a wrapper around one of them is stripped, and a folder with that name is a game path, not a wrapper.
    pub(in crate::import::install) fn new(deploy: &'a Deploy, base_roots: &'a [String]) -> Self {
        let slots: Vec<SlotRule<'a>> = deploy
            .slots
            .iter()
            .map(|slot| SlotRule {
                slot,
                dir: split_dir(&slot.dir),
            })
            .collect();
        let mirrors: Vec<MirrorRule<'a>> = deploy
            .mirror
            .iter()
            .map(|mirror| MirrorRule {
                mirror,
                dir: split_dir(&mirror.dir),
            })
            .collect();
        let mut mirrors_by_first: Vec<(&str, Vec<usize>)> = Vec::new();
        for (at, rule) in mirrors.iter().enumerate() {
            let Some(&first) = rule.dir.first() else {
                continue;
            };
            if let Some((_, indices)) = mirrors_by_first
                .iter_mut()
                .find(|(known, _)| known.eq_ignore_ascii_case(first))
            {
                indices.push(at);
            } else {
                mirrors_by_first.push((first, vec![at]));
            }
        }
        let protected: Vec<Vec<&str>> = deploy.protected.iter().map(|dir| split_dir(dir)).collect();
        let aliases: Vec<(&str, Vec<&str>)> = deploy
            .aliases
            .iter()
            .map(|(name, target)| {
                let parts: Vec<&str> = target.split('/').filter(|part| !part.is_empty()).collect();
                (name.as_str(), parts)
            })
            .collect();
        let mut place_folders: Vec<&str> = Vec::new();
        let places = deploy.slots.iter().map(|slot| slot.dir.as_str());
        let places = places
            .chain(deploy.mirror.iter().map(|mirror| mirror.dir.as_str()))
            .chain(deploy.protected.iter().map(String::as_str));
        for first in places.filter_map(|dir| dir.split('/').next()) {
            if !place_folders
                .iter()
                .any(|known| known.eq_ignore_ascii_case(first))
            {
                place_folders.push(first);
            }
        }
        for root in base_roots {
            if !place_folders
                .iter()
                .any(|known| known.eq_ignore_ascii_case(root))
            {
                place_folders.push(root.as_str());
            }
        }
        let mut game_folders = place_folders.clone();
        for first in deploy
            .aliases
            .keys()
            .filter_map(|name| name.split('/').next())
        {
            if !game_folders
                .iter()
                .any(|known| known.eq_ignore_ascii_case(first))
            {
                game_folders.push(first);
            }
        }
        Self {
            deploy,
            slots,
            mirrors,
            mirrors_by_first,
            protected,
            aliases,
            place_folders,
            game_folders,
        }
    }

    /// Rewrites the first folder of an entry by the definition's alias for it: `plugins/<Mod>/x.dll` becomes
    /// `BepInEx/plugins/<Mod>/x.dll`. An entry whose first part is only a file, or has no alias, keeps its parts.
    pub(in crate::import::install) fn rewrite_aliases<'p>(&self, parts: &mut Vec<&'p str>)
    where
        'a: 'p,
    {
        let [first, rest @ ..] = parts.as_slice() else {
            return;
        };
        if rest.is_empty() {
            return;
        }
        let Some((_, target)) = self
            .aliases
            .iter()
            .find(|(name, _)| first.eq_ignore_ascii_case(name))
        else {
            return;
        };
        let mut rewritten: Vec<&'p str> = target.clone();
        rewritten.extend_from_slice(rest);
        *parts = rewritten;
    }

    pub(in crate::import::install) fn wrapper_depth(&self, files: &[(usize, Vec<&str>)]) -> usize {
        let mut top: Option<&str> = None;
        let mut inside = false;
        let mut repeated = false;
        for (_, parts) in files {
            let [first, second, ..] = parts.as_slice() else {
                continue;
            };
            match top {
                None => top = Some(first),
                Some(known) if !known.eq_ignore_ascii_case(first) => return 0,
                Some(_) => {}
            }
            inside |= self
                .game_folders
                .iter()
                .any(|known| known.eq_ignore_ascii_case(second));
            repeated |= first.eq_ignore_ascii_case(second);
        }
        let Some(top) = top else {
            return 0;
        };
        // A folder that is itself a deploy place is a game path, not a wrapper: `BepInEx/plugins` is the game's
        // plugins folder, not a pack inside `BepInEx/`. A doubled root (`BepInEx/BepInEx/...`) still has an outer
        // wrapper, so it keeps the one level the archive added.
        if self
            .place_folders
            .iter()
            .any(|known| known.eq_ignore_ascii_case(top))
        {
            return usize::from(repeated);
        }
        // A version folder (`1.21.4/`, `v0.2.6/`) is an archive wrapper even when the folder inside is a new
        // framework root: `1.21.4/red4ext/...` is `red4ext/...`.
        if is_version_folder(top) {
            return 1;
        }
        // A folder that holds a file named after itself (`red4ext/RED4ext.dll`, `BepInEx/core/BepInEx.dll`) is a
        // framework root, not a wrapper.
        let same_name = files.iter().any(|(_, parts)| {
            parts.len() > 1 && parts[1..].iter().any(|part| stem_eq_ignore_case(part, top))
        });
        if same_name {
            return 0;
        }
        usize::from(inside)
    }

    /// Decides where an entry goes: its group, its slot (empty for a mirrored file) and its path in the library folder.
    /// `None` means no rule takes it, and it is skipped.
    pub(in crate::import::install) fn classify(
        &self,
        parts: &[&str],
        index: &UnitIndex<'_>,
        _units: &[Unit<'_>],
        archive_name: &str,
    ) -> Result<Option<(String, String, String)>, InstallError> {
        let whole = || archive_name.to_string();
        if let Some(rel) = self.mirrored(parts) {
            return Ok(Some((whole(), String::new(), rel)));
        }
        // A bare file named as a root file keeps that name in the deployment folder. It comes before the unit roots and
        // the slots, so a marker at the archive root cannot take it.
        if let [file] = parts
            && let Some(name) = self.deploy.root_file_of(file)
        {
            return Ok(Some((whole(), String::new(), name.to_string())));
        }
        // A bare file with a move rule goes to the folder the game reads it from, for example a loose `.archive` to
        // `archive/pc/mod`.
        if let [file] = parts
            && let Some(rel) = self.deploy.move_destination(file)
        {
            return Ok(Some((whole(), String::new(), rel)));
        }
        let in_unit = |unit: &Unit<'_>| -> Option<(String, String, String)> {
            if parts.len() <= unit.root.len()
                || !starts_with_path(parts, &unit.root)
                || !extension_allowed(parts, &unit.slot.extensions)
            {
                return None;
            }
            let group = unit
                .root
                .last()
                .map_or_else(whole, |name| (*name).to_string());
            let rel = parts[unit.root.len()..].join("/");
            Some((group, unit.slot.dir.clone(), rel))
        };
        // The deepest matching root first; a root whose slot does not allow the extension lets a shorter one take
        // the file, in the same rule order. Only the roots that are a prefix of this path are tried.
        if let Some(found) = index.candidates(parts).flatten().copied().find_map(in_unit) {
            return Ok(Some(found));
        }
        for rule in &self.slots {
            if parts.len() <= rule.dir.len()
                || !starts_with_path(parts, &rule.dir)
                || !extension_allowed(parts, &rule.slot.extensions)
            {
                continue;
            }
            // `Mods/<mod>/...` is a mod folder; a file directly in the slot folder belongs to the archive.
            let rest = &parts[rule.dir.len()..];
            let (group, rel) = match rest {
                [file] => (whole(), (*file).to_string()),
                [folder, inner @ ..] => ((*folder).to_string(), inner.join("/")),
                [] => continue,
            };
            return Ok(Some((group, rule.slot.dir.clone(), rel)));
        }
        for dir in &self.protected {
            if parts.len() > dir.len() && starts_with_path(parts, dir) {
                return Err(InstallError::UnsupportedRoot {
                    root: parts[..dir.len()].join("/"),
                });
            }
        }
        if let [file] = parts
            && let Some(rule) = self.slots.iter().find(|rule| {
                !rule.slot.root_extensions.is_empty()
                    && extension_allowed(parts, &rule.slot.root_extensions)
            })
        {
            return Ok(Some((whole(), rule.slot.dir.clone(), (*file).to_string())));
        }
        // A mirrored folder one level down, in an archive with more than one top folder.
        if let [_, inner @ ..] = parts
            && let Some(rel) = self.mirrored(inner)
        {
            return Ok(Some((whole(), String::new(), rel)));
        }
        if self.deploy.full_root && parts.len() > 1 {
            return Ok(Some((whole(), String::new(), parts.join("/"))));
        }
        Ok(None)
    }

    /// The game path of a file under a mirrored folder, with the folder in the definition's spelling. A folder directly
    /// under it gets the mirror's prefix (`DLC/Corvo/...` becomes `DLC/dlcCorvo/...`).
    pub(super) fn mirrored(&self, parts: &[&str]) -> Option<String> {
        let first = parts.first()?;
        let (_, indices) = self
            .mirrors_by_first
            .iter()
            .find(|(known, _)| first.eq_ignore_ascii_case(known))?;
        for &at in indices {
            let rule = &self.mirrors[at];
            if starts_with_path(parts, &rule.dir)
                && extension_allowed(parts, &rule.mirror.extensions)
            {
                return Some(mirror_path(rule, parts));
            }
        }
        None
    }
}

fn split_dir(dir: &str) -> Vec<&str> {
    dir.split('/').collect()
}

/// Whether a top folder is an archive version wrapper: `1.21.4`, `v0.2.6`, `2024`. Only digits and dots follow an
/// optional `v`.
fn is_version_folder(name: &str) -> bool {
    let digits = name
        .strip_prefix(['v', 'V'])
        .unwrap_or(name)
        .trim_end_matches('.');
    !digits.is_empty()
        && digits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
        && digits.bytes().any(|byte| byte.is_ascii_digit())
}

/// Whether a path component's stem equals `name`, ignoring ASCII case: `RED4ext.dll` matches `red4ext`.
fn stem_eq_ignore_case(part: &str, name: &str) -> bool {
    let stem = part.rsplit_once('.').map_or(part, |(stem, _)| stem);
    stem.eq_ignore_ascii_case(name)
}
