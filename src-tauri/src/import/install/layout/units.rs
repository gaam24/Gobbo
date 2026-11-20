//! The mod roots of an archive: finding them by the game's markers, and indexing them by path prefix.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::games::Slot;

use super::classify::{extension_allowed, starts_with_path};
use super::plan::LayoutPlan;

/// A mod root found by a slot's marker, for example `Mods/modlet` for `Mods/modlet/ModInfo.xml`, or by a slot's root
/// folder extensions, for example `Plugin` for `Plugin/plugin.dll`.
pub(in crate::import::install) struct Unit<'a> {
    pub(in crate::import::install) slot: &'a Slot,
    /// The parts of the root, in their archive spelling. Empty when the marker is at the archive root.
    pub(in crate::import::install) root: Vec<&'a str>,
}

impl LayoutPlan<'_> {
    /// Finds mod roots by the slots' markers. A marker is a file or folder path; the root is the part of the entry path
    /// before it. Roots inside a mirrored or protected folder are not mods. The deepest root comes first, so nested
    /// roots take their own files.
    pub(in crate::import::install) fn unit_roots<'b>(
        &'b self,
        files: &'b [(usize, Vec<&'b str>)],
    ) -> Vec<Unit<'b>> {
        let mut units: Vec<Unit<'b>> = Vec::new();

        for rule in &self.slots {
            let Some(marker) = &rule.slot.marker else {
                continue;
            };
            let marker: Vec<&str> = marker.split('/').filter(|part| !part.is_empty()).collect();

            if marker.is_empty() {
                continue;
            }
            // Roots repeat for every file of a mod. A set of normalized roots replaces a scan of the units found so
            // far, which was quadratic in the number of mods.
            let mut seen: FxHashSet<String> = FxHashSet::default();
            // Entries of one mod are usually next to each other. Remembering the last root avoids building and
            // hashing the same normalized path for every file of that mod.
            let mut last: Option<Vec<&str>> = None;

            for (_, parts) in files {
                let found = (0..=parts.len().saturating_sub(marker.len()))
                    .find(|&at| starts_with_path(&parts[at..], &marker));
                let Some(at) = found else {
                    continue;
                };
                let root = &parts[..at];
                let reserved = self
                    .mirrors
                    .iter()
                    .any(|rule| starts_with_path(root, &rule.dir))
                    || self.protected.iter().any(|dir| starts_with_path(root, dir));
                if reserved || last.as_deref() == Some(root) {
                    continue;
                }

                if seen.insert(root_key(root)) {
                    units.push(Unit {
                        slot: rule.slot,
                        root: root.to_vec(),
                    });
                }

                last = Some(root.to_vec());
            }
        }

        units.extend(self.root_folder_units(files));
        units.sort_by_key(|unit| std::cmp::Reverse(unit.root.len()));
        units
    }

    /// The folders at the archive root that hold a file of a slot's `root_folder_extensions`: `<Plugin>/<plugin>.dll` is
    /// one mod, and its folder goes into the slot. In a flat slot the folder only names the mod; its files deploy by
    /// name into the slot. A folder that is a game folder (`BepInEx/x.dll`) is not a mod.
    fn root_folder_units<'b>(&'b self, files: &'b [(usize, Vec<&'b str>)]) -> Vec<Unit<'b>> {
        let mut units: Vec<Unit<'b>> = Vec::new();

        for rule in &self.slots {
            if rule.slot.root_folder_extensions.is_empty() {
                continue;
            }

            let mut seen: FxHashSet<String> = FxHashSet::default();

            for (_, parts) in files {
                let [folder, _] = parts.as_slice() else {
                    continue;
                };

                if !extension_allowed(parts, &rule.slot.root_folder_extensions)
                    || self
                        .game_folders
                        .iter()
                        .any(|known| known.eq_ignore_ascii_case(folder))
                {
                    continue;
                }

                if seen.insert(root_key(&[*folder])) {
                    units.push(Unit {
                        slot: rule.slot,
                        root: vec![*folder],
                    });
                }
            }
        }

        units
    }
}

/// The mod roots by their normalized path, so an entry does not compare with every root of every mod.
pub(in crate::import::install) struct UnitIndex<'u> {
    by_root: FxHashMap<String, Vec<&'u Unit<'u>>>,
    /// A marker that sits at the archive root has an empty root; it is the last rule.
    at_root: Vec<&'u Unit<'u>>,
    max_len: usize,
}

impl<'u> UnitIndex<'u> {
    pub(in crate::import::install) fn new(units: &'u [Unit<'u>]) -> Self {
        let mut by_root: FxHashMap<String, Vec<&'u Unit<'u>>> = FxHashMap::default();
        let mut at_root = Vec::new();
        let mut max_len = 0;

        for unit in units {
            if unit.root.is_empty() {
                at_root.push(unit);
                continue;
            }

            max_len = max_len.max(unit.root.len());
            by_root.entry(root_key(&unit.root)).or_default().push(unit);
        }

        Self {
            by_root,
            at_root,
            max_len,
        }
    }

    /// The units that can take a path, in rule order: the longest root prefix first, then each shorter one, and the
    /// archive-root markers last.
    pub(in crate::import::install) fn candidates<'p, 'q>(
        &'p self,
        parts: &'p [&'q str],
    ) -> UnitCandidates<'u, 'p, 'q> {
        UnitCandidates {
            index: self,
            parts,
            len: self.max_len.min(parts.len().saturating_sub(1)),
            key: String::new(),
            at_root: false,
        }
    }
}

/// The prefix buckets of one path, longest root first, then the archive-root markers. Within a bucket the units keep
/// the order they were found in.
pub(in crate::import::install) struct UnitCandidates<'u, 'p, 'q> {
    index: &'p UnitIndex<'u>,
    parts: &'p [&'q str],
    len: usize,
    key: String,
    at_root: bool,
}

impl<'u, 'p> Iterator for UnitCandidates<'u, 'p, '_> {
    type Item = &'p [&'u Unit<'u>];

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.index;

        while self.len > 0 {
            let len = self.len;
            self.len -= 1;
            prefix_key(self.parts, len, &mut self.key);

            if let Some(bucket) = index.by_root.get(self.key.as_str()) {
                return Some(bucket.as_slice());
            }
        }

        if self.at_root {
            return None;
        }

        self.at_root = true;

        if index.at_root.is_empty() {
            return None;
        }

        Some(index.at_root.as_slice())
    }
}

fn root_key(root: &[&str]) -> String {
    let mut key = String::new();

    for (at, part) in root.iter().enumerate() {
        if at > 0 {
            key.push('/');
        }

        push_lowercase(&mut key, part);
    }

    key
}

fn prefix_key(parts: &[&str], len: usize, key: &mut String) {
    key.clear();

    for (at, part) in parts[..len].iter().enumerate() {
        if at > 0 {
            key.push('/');
        }

        push_lowercase(key, part);
    }
}

/// Lowercase only ASCII, as `eq_ignore_ascii_case` does; the rest of the path keeps its spelling.
fn push_lowercase(key: &mut String, text: &str) {
    key.extend(text.chars().map(|character| character.to_ascii_lowercase()));
}
