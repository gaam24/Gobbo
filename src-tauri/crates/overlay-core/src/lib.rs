//! The layers of an overlay deployment, as Mod Organizer 2 and overlayfs use them. One mount has a `root` (the base
//! the program sees), an optional `staging` (the mods, or nothing for a user-data redirect) and an `upper` (the
//! profile's writes). The game mount uses the game folder as `root` and the mod layer as `staging`; a user-data mount
//! uses the real folder (or an empty base) as `root` and the profile's layer as `upper`. Deletions of files in the
//! lower layers are kept as whiteouts, so they hide the file without touching it. `MountSpec`/`MountConfig` describe
//! the whole set; Gobbo writes it and the Windows hook reads it. This crate is plain logic with no Windows calls, so it
//! is tested on every platform. The one `cfg(windows)` helper, `wide`, only converts text into a NUL-terminated UTF-16
//! buffer for the Win32 boundary. The Windows hook and Gobbo both use it.

#![forbid(unsafe_code)]

pub mod early;
pub mod nt_listing;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

/// Environment variables that pass the layers from Gobbo to the hook in the game process. Only `to_env` and
/// `from_env` spell them; a session with more than the game mount uses the `MountConfig` file named by `ENV_MOUNTS`.
const ENV_ROOT: &str = "GOBBO_OVERLAY_ROOT";
const ENV_STAGING: &str = "GOBBO_OVERLAY_STAGING";
const ENV_UPPER: &str = "GOBBO_OVERLAY_UPPER";
/// The JSON file that names every mount of the session, including the game folder and the profile's user-data roots.
/// A hook that finds it uses it instead of the three single-layer variables above.
pub const ENV_MOUNTS: &str = "GOBBO_OVERLAY_MOUNTS";

/// The three folders of one overlay session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layers {
    /// The folder the program sees: the bottom layer of the view.
    pub root: PathBuf,
    /// The mod layer, above the base. `None` for a user-data redirect, which has no mod staging layer.
    pub staging: Option<PathBuf>,
    /// The profile's writes, above everything else.
    pub upper: PathBuf,
}

impl Layers {
    /// The three folders of one overlay session.
    pub fn new(root: PathBuf, staging: Option<PathBuf>, upper: PathBuf) -> Self {
        Self {
            root,
            staging,
            upper,
        }
    }
}

/// One mount in the hook's session: the path the program sees, the lower base, and the profile's upper layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountSpec {
    /// `game` for the game folder, otherwise the user-data set id from `game.toml`.
    pub id: String,
    /// The path the program uses.
    pub guest: PathBuf,
    /// The read-only base the mount serves. For a user-data redirect without inheritance this is an empty folder.
    pub root: PathBuf,
    /// The mod layer of the game mount; `None` for a user-data redirect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staging: Option<PathBuf>,
    /// The profile's writes.
    pub upper: PathBuf,
}

impl MountSpec {
    /// The id of the game folder's mount. Gobbo writes it when it builds the file, and `MountConfig::game` looks for
    /// it, so the spelling of the id lives in one place.
    pub const GAME_ID: &str = "game";

    /// The layers of this mount.
    pub fn layers(&self) -> Layers {
        Layers::new(self.root.clone(), self.staging.clone(), self.upper.clone())
    }
}

/// The most mounts a session accepts: the game and seven user-data sets. The pass-through path checks every mount, so
/// the list stays small on purpose.
pub const MAX_MOUNTS: usize = 8;

/// Why a mount set was refused. A bad set fails the launch instead of running the game against the real folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidMountConfig {
    reason: &'static str,
}

impl std::fmt::Display for InvalidMountConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid mount config: {}", self.reason)
    }
}

impl std::error::Error for InvalidMountConfig {}

/// The complete mount set of a session, written by Gobbo and read by the hook before the game runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountConfig {
    /// The format version. The hook refuses a version it does not understand instead of guessing.
    version: u32,
    /// The game mount first, then the user-data redirects.
    mounts: Vec<MountSpec>,
}

impl MountConfig {
    /// The current format version.
    pub const VERSION: u32 = 1;

    /// Builds a mount set and refuses one the hook cannot use safely. Gobbo and the hook both go through this or
    /// [`MountConfig::validate`], so an unchecked set never reaches a game.
    pub fn try_new(version: u32, mounts: Vec<MountSpec>) -> Result<Self, InvalidMountConfig> {
        Self { version, mounts }.validate()
    }

    /// Checks a mount set read from a file, returning it unchanged when the hook can use it. A bad set fails the launch
    /// instead of running the game against the real folder.
    pub fn validate(self) -> Result<Self, InvalidMountConfig> {
        if self.version != Self::VERSION {
            return Err(InvalidMountConfig {
                reason: "unsupported version",
            });
        }

        if self.mounts.len() > MAX_MOUNTS {
            return Err(InvalidMountConfig {
                reason: "too many mounts",
            });
        }

        if !self
            .mounts
            .first()
            .is_some_and(|mount| mount.id == MountSpec::GAME_ID)
        {
            return Err(InvalidMountConfig {
                reason: "the game mount must be first",
            });
        }

        let mut ids = BTreeSet::new();

        for mount in &self.mounts {
            if mount.id.is_empty() {
                return Err(InvalidMountConfig {
                    reason: "empty mount id",
                });
            }

            if !ids.insert(mount.id.as_str()) {
                return Err(InvalidMountConfig {
                    reason: "duplicate mount id",
                });
            }

            if !mount.guest.is_absolute() {
                return Err(InvalidMountConfig {
                    reason: "guest path is not absolute",
                });
            }

            if !mount.root.is_absolute() {
                return Err(InvalidMountConfig {
                    reason: "root path is not absolute",
                });
            }

            if !mount.upper.is_absolute() {
                return Err(InvalidMountConfig {
                    reason: "upper path is not absolute",
                });
            }

            if mount
                .staging
                .as_ref()
                .is_some_and(|path| !path.is_absolute())
            {
                return Err(InvalidMountConfig {
                    reason: "staging path is not absolute",
                });
            }
        }

        Ok(self)
    }

    /// The mounts of this session, the game first and then the user-data redirects.
    pub fn mounts(&self) -> &[MountSpec] {
        &self.mounts
    }

    /// The game mount, which is first and carries `MountSpec::GAME_ID`. The shape is validated before this is read, so
    /// this only selects and never validates.
    pub fn game(&self) -> Option<&MountSpec> {
        let [game, ..] = &self.mounts[..] else {
            return None;
        };

        (game.id == MountSpec::GAME_ID).then_some(game)
    }

    /// The user-data redirects after the game mount.
    pub fn redirects(&self) -> impl Iterator<Item = &MountSpec> {
        self.mounts.iter().skip(1)
    }
}

/// The layer a path resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Upper,
    Staging,
    Root,
}

/// The paths the game deleted from the lower layers, relative to the game folder and in lower case. A path hides the
/// lower-layer entries at it and below it. The upper layer is never hidden, so a file the game writes again is visible.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Whiteouts {
    paths: BTreeSet<String>,
    /// A hash of every lower-case key in `paths`, for a fast negative answer without allocating the lower-case query.
    /// A collision only costs the exact lookup that `paths` does next, so correctness does not depend on the hash.
    hashes: FxHashSet<u64>,
}

impl Whiteouts {
    /// The whiteouts from their file: one relative path per line. Blank lines are skipped.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut whiteouts = Self::default();

        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            whiteouts.hide(line);
        }

        whiteouts
    }

    /// The text of the whiteouts file: one relative path per line.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::with_capacity(self.paths.len() * 32);

        for path in &self.paths {
            text.push_str(path);
            text.push('\n');
        }

        text
    }

    /// Whether there are no whiteouts at all. A listing uses this to skip building a path for every lower entry.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// The hidden paths, relative to the mount, in the order the set keeps them. A caller that moves the data back out
    /// of a profile layer needs the paths, not only the visibility check.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.paths.iter().map(String::as_str)
    }

    /// Whether `rel` is hidden, because it or one of its folders is a whiteout. The game folder itself is never hidden.
    pub fn hides(&self, rel: &str) -> bool {
        if rel.is_empty() || self.paths.is_empty() {
            return false;
        }
        // ASCII is the common case and needs no lower-case allocation: the set keeps lower-case keys and
        // `contains_folded` compares the query case-insensitively. The hash is built once from left to right, so a
        // deep path hashes its first folder once instead of once per prefix. A non-ASCII path keeps the exact old
        // behavior.
        if rel.is_ascii() {
            let mut hash = FNV_OFFSET;

            for (index, byte) in rel.bytes().enumerate() {
                if byte == b'\\' && self.hash_hides(hash, &rel[..index]) {
                    return true;
                }

                hash = fnv_step(hash, byte.to_ascii_lowercase());
            }
            // The whole path, including a trailing separator when the caller gave one.
            return self.hash_hides(hash, rel);
        }

        let mut buffer = [0u8; 1024];

        if let Some(lower) = fold_key_into(rel, &mut buffer) {
            return self.hides_folded(lower);
        }

        self.hides_folded(&fold_key(rel))
    }

    /// `hides_child` when the caller already knows that `rel` itself is not hidden.
    pub fn hides_child_in_visible_folder(&self, rel: &str, name: &str) -> bool {
        if self.paths.is_empty() {
            return false;
        }

        if rel.is_empty() {
            return self.contains_folded(name);
        }

        if rel.is_ascii() && name.is_ascii() {
            let length = rel.len() + 1 + name.len();
            let mut buffer = [0u8; 512];

            if length <= buffer.len() {
                buffer[..rel.len()].copy_from_slice(rel.as_bytes());
                buffer[rel.len()] = b'\\';
                buffer[rel.len() + 1..length].copy_from_slice(name.as_bytes());

                if let Ok(path) = std::str::from_utf8(&buffer[..length]) {
                    return self.contains_folded(path);
                }
            }
        }

        let mut buffer = [0u8; 1024];

        if let Some(path) = fold_child_into(rel, name, &mut buffer) {
            return self.hash_hides_folded(key_hash(path), path);
        }

        self.contains_folded(&join_rel(rel, name))
    }

    /// Whether the entry `name` inside the visible folder `rel` is hidden, for a name that is still UTF-16 as a
    /// directory listing receives it. An ASCII name is converted to a small stack buffer, so a lower entry does not
    /// allocate just to ask about a whiteout. A directory listing checks the folder once and then asks this for every
    /// entry, so the folder prefixes are not walked again per entry.
    pub fn hides_child_units_in_visible_folder(&self, rel: &str, name: &[u16]) -> bool {
        if self.paths.is_empty() {
            return false;
        }

        if units_are_ascii(name) {
            let mut ascii = [0u8; 260];
            let length = name.len().min(ascii.len());

            if length == name.len() {
                for (slot, unit) in ascii.iter_mut().zip(name) {
                    *slot = *unit as u8;
                }

                if let Ok(name) = std::str::from_utf8(&ascii[..length]) {
                    return self.hides_child_in_visible_folder(rel, name);
                }
            }
        }

        let mut buffer = [0u8; 1024];

        if let Some(path) = fold_child_units_into(rel, name, &mut buffer) {
            return self.hash_hides_folded(key_hash(path), path);
        }

        let name = String::from_utf16_lossy(name);
        self.hides_child_in_visible_folder(rel, &name)
    }

    /// Whether a prefix whose ASCII hash is already known is present. The hash answers a miss without building the
    /// lower-case string; only a possible hit falls back to the exact lookup in `paths`.
    fn hash_hides(&self, hash: u64, prefix: &str) -> bool {
        self.hashes.contains(&hash) && self.paths.contains(&prefix.to_ascii_lowercase())
    }

    /// Whether a key that is already folded is present, with its known hash.
    fn hash_hides_folded(&self, hash: u64, folded: &str) -> bool {
        self.hashes.contains(&hash) && self.paths.contains(folded)
    }

    /// `hides` for a path that is already folded, as the non-ASCII branch builds it.
    fn hides_folded(&self, lower: &str) -> bool {
        let mut hash = FNV_OFFSET;

        for (index, byte) in lower.bytes().enumerate() {
            if byte == b'\\' && self.hash_hides_folded(hash, &lower[..index]) {
                return true;
            }

            hash = fnv_step(hash, byte);
        }

        self.hash_hides_folded(hash, lower)
    }

    /// Whether a key is present, ignoring ASCII case. The hash set answers a miss without building the lower-case
    /// string; only a possible hit falls back to the exact lower-case lookup in `paths`.
    fn contains_folded(&self, key: &str) -> bool {
        if key.is_ascii() {
            return self.hash_hides(key_hash(key), key);
        }

        let mut buffer = [0u8; 1024];

        if let Some(lower) = fold_key_into(key, &mut buffer) {
            return self.hash_hides_folded(key_hash(lower), lower);
        }

        let lower = fold_key(key);
        self.hash_hides_folded(key_hash(&lower), &lower)
    }

    /// Whether a whiteout can hide the folder `rel` or one of its immediate children. A deeper whiteout does not change
    /// this folder's own listing; it matters when the child that holds it is listed.
    pub fn affects_listing(&self, rel: &str) -> bool {
        if self.paths.is_empty() {
            return false;
        }

        if self.hides(rel) {
            return true;
        }

        let lower = fold_key(rel);
        let prefix = if lower.is_empty() {
            String::new()
        } else {
            format!("{lower}\\")
        };
        // The paths with a prefix are a contiguous range in the ordered set, so an unrelated whiteout does not pay a
        // full scan.
        self.paths
            .range(prefix.clone()..)
            .take_while(|path| path.starts_with(&prefix))
            .any(|path| !path[prefix.len()..].contains('\\'))
    }

    /// Hides `rel` in the lower layers, as a deletion does.
    pub fn hide(&mut self, rel: &str) {
        let lower = fold_key(rel);
        self.hashes.insert(key_hash(&lower));
        self.paths.insert(lower);
    }
}

impl Layers {
    /// The environment variables that describe these layers. They carry one mount (the game mount) for the legacy
    /// `gobbo run` path; a session with user-data redirects uses `MountConfig` instead.
    pub fn to_env(&self) -> [(&'static str, String); 3] {
        [
            (ENV_ROOT, self.root.display().to_string()),
            (
                ENV_STAGING,
                self.staging
                    .as_ref()
                    .map_or_else(String::new, |path| path.display().to_string()),
            ),
            (ENV_UPPER, self.upper.display().to_string()),
        ]
    }

    /// The layers from environment variables, read through `get` (the process environment in production).
    /// `None` when one of the three is missing or empty.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let path = |name: &str| {
            get(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        Some(Self {
            root: path(ENV_ROOT)?,
            staging: Some(path(ENV_STAGING)?),
            upper: path(ENV_UPPER)?,
        })
    }

    /// The folder of one layer. The upper layer is `upper`, the mod layer `staging` and the base folder `root`.
    /// `staging` is `None` for a mount without a mod layer.
    pub fn base(&self, layer: Layer) -> Option<&Path> {
        match layer {
            Layer::Upper => Some(&self.upper),
            Layer::Staging => self.staging.as_deref(),
            Layer::Root => Some(&self.root),
        }
    }

    /// The layers this mount has, top first. A mount without staging skips it instead of probing an empty folder.
    pub fn layer_paths(&self) -> impl Iterator<Item = (Layer, &Path)> {
        [Layer::Upper, Layer::Staging, Layer::Root]
            .into_iter()
            .filter_map(|layer| self.base(layer).map(|base| (layer, base)))
    }

    /// The top-most layer where the relative path exists, with the full path in that layer. `exists` tells whether a
    /// path exists on disk. Whiteouts are not considered; see `resolve_visible`.
    pub fn resolve(&self, rel: &str, exists: impl Fn(&Path) -> bool) -> Option<(Layer, PathBuf)> {
        self.resolve_visible(rel, exists, &Whiteouts::default())
    }

    /// Like `resolve`, but the lower layers are skipped for a path that the whiteouts hide. The upper layer always counts.
    pub fn resolve_visible(
        &self,
        rel: &str,
        exists: impl Fn(&Path) -> bool,
        whiteouts: &Whiteouts,
    ) -> Option<(Layer, PathBuf)> {
        let upper = join(&self.upper, rel);

        if exists(&upper) {
            return Some((Layer::Upper, upper));
        }

        if whiteouts.hides(rel) {
            return None;
        }

        [Layer::Staging, Layer::Root]
            .into_iter()
            .filter_map(|layer| self.base(layer).map(|base| (layer, base)))
            .map(|(layer, base)| (layer, join(base, rel)))
            .find(|(_, path)| exists(path))
    }

    /// Where a write to the relative path goes. Writes never reach the staging or the game folder.
    pub fn write_target(&self, rel: &str) -> PathBuf {
        join(&self.upper, rel)
    }

    /// The whiteouts file of these layers. It sits next to the upper layer (`upper.whiteouts`), so it is outside both
    /// the game folder and the mod layer. Gobbo and the hook both use this name.
    pub fn whiteouts_file(&self) -> PathBuf {
        self.upper.with_extension("whiteouts")
    }

    /// The journal of whiteout changes since the last compaction. It sits next to the whiteouts file
    /// (`upper.whiteouts.journal`). The hook appends one path per line and folds the journal into the snapshot when it
    /// grows; a rebuilt layer removes both files.
    pub fn whiteouts_journal_file(&self) -> PathBuf {
        self.upper.with_extension("whiteouts.journal")
    }
}

/// The prefix and the suffix of the temporary file a copy-up writes beside its target before it publishes it. A name
/// with this shape is never a game file: the hook hides it from listings, and Gobbo removes the ones an interrupted copy
/// left behind.
const COPY_TEMP_PREFIX: &str = ".gobbo-copy-";
const COPY_TEMP_SUFFIX: &str = ".tmp";
/// The separator between the process id and the sequence in a copy-up temporary name.
const COPY_TEMP_SEP: char = '-';

/// The name of the temporary copy of one copy-up: the process that makes it and a counter that is unique in it.
pub fn copy_temp_name(process: u32, sequence: u64) -> String {
    format!("{COPY_TEMP_PREFIX}{process}{COPY_TEMP_SEP}{sequence}{COPY_TEMP_SUFFIX}")
}

/// Whether a name has the shape of a copy-up temporary file (see `copy_temp_name`).
pub fn is_copy_temp_name(name: &str) -> bool {
    let Some(middle) = name
        .strip_prefix(COPY_TEMP_PREFIX)
        .and_then(|rest| rest.strip_suffix(COPY_TEMP_SUFFIX))
    else {
        return false;
    };
    middle
        .split_once(COPY_TEMP_SEP)
        .is_some_and(|(process, sequence)| {
            process.parse::<u32>().is_ok() && sequence.parse::<u64>().is_ok()
        })
}

/// The folded form of one character: the rule every name comparison of Gobbo uses, in the hook and in the app. ASCII folds
/// as Windows folds it. Any other letter takes its lower-case form when that form is one character (`ś` for `Ś`); a
/// letter whose lower case is several characters keeps its own form. The same rule on both platforms is what lets a
/// name the program spells in another case find its file.
pub fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }

    let mut lower = c.to_lowercase();

    match (lower.next(), lower.next()) {
        (Some(one), None) => one,
        _ => c,
    }
}

/// The ASCII branch of `fold_unit`: folds an ASCII unit without touching any other unit. The hook uses it to recognize
/// drive letters and keywords in a path, where a Unicode fold must not turn another letter into ASCII.
pub const fn fold_ascii_unit(unit: u16) -> u16 {
    if unit >= 0x41 && unit <= 0x5A {
        unit + 32
    } else {
        unit
    }
}

/// The folded form of one UTF-16 code unit, as `fold_char` folds a character. A surrogate is not a character and keeps
/// its unit.
pub fn fold_unit(unit: u16) -> u16 {
    if unit < 0x80 {
        return fold_ascii_unit(unit);
    }

    char::from_u32(u32::from(unit))
        .and_then(|c| u16::try_from(u32::from(fold_char(c))).ok())
        .unwrap_or(unit)
}

/// A string as a NUL-terminated UTF-16 buffer, for Win32. Windows only: the hook and the launcher use it at the API
/// boundary.
#[cfg(windows)]
pub fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Whether every UTF-16 unit is ASCII, as the no-allocation path requires.
fn units_are_ascii(name: &[u16]) -> bool {
    name.iter().all(|unit| *unit < 0x80)
}

/// The folded form of a whole name, as `fold_char` folds each character.
pub fn fold_key(text: &str) -> String {
    if text.is_ascii() {
        return text.to_ascii_lowercase();
    }

    text.chars().map(fold_char).collect()
}

/// `fold_key` into a caller's buffer, without allocating. `None` when the folded text does not fit.
pub fn fold_key_into<'a>(text: &str, buffer: &'a mut [u8]) -> Option<&'a str> {
    if text.is_ascii() {
        if text.len() > buffer.len() {
            return None;
        }

        for (slot, byte) in buffer.iter_mut().zip(text.bytes()) {
            *slot = byte.to_ascii_lowercase();
        }

        return std::str::from_utf8(&buffer[..text.len()]).ok();
    }

    let mut length = 0;

    for c in text.chars() {
        let mut bytes = [0u8; 4];
        let encoded = fold_char(c).encode_utf8(&mut bytes);

        if length + encoded.len() > buffer.len() {
            return None;
        }

        buffer[length..length + encoded.len()].copy_from_slice(encoded.as_bytes());
        length += encoded.len();
    }

    std::str::from_utf8(&buffer[..length]).ok()
}

/// Folds `rel\name` into `buffer`; `rel` may be empty. `None` when it does not fit.
fn fold_child_into<'a>(rel: &str, name: &str, buffer: &'a mut [u8]) -> Option<&'a str> {
    let mut length = 0;
    fold_rel(rel, buffer, &mut length)?;

    for c in name.chars() {
        if !push_fold(c, buffer, &mut length) {
            return None;
        }
    }

    std::str::from_utf8(&buffer[..length]).ok()
}

/// Folds `rel\name` when `name` is still UTF-16, as a directory listing receives it. `None` when a unit is not a
/// character or the text does not fit; the caller then uses the lossy path.
fn fold_child_units_into<'a>(rel: &str, name: &[u16], buffer: &'a mut [u8]) -> Option<&'a str> {
    let mut length = 0;
    fold_rel(rel, buffer, &mut length)?;

    for c in char::decode_utf16(name.iter().copied()) {
        if !push_fold(c.ok()?, buffer, &mut length) {
            return None;
        }
    }

    std::str::from_utf8(&buffer[..length]).ok()
}

/// Folds `rel` and its trailing separator into `buffer`. `None` when it does not fit.
fn fold_rel(rel: &str, buffer: &mut [u8], length: &mut usize) -> Option<()> {
    for c in rel.chars() {
        if !push_fold(c, buffer, length) {
            return None;
        }
    }

    if !rel.is_empty() {
        push_byte(b'\\', buffer, length).then_some(())?;
    }

    Some(())
}

/// Appends one folded character as UTF-8.
fn push_fold(c: char, buffer: &mut [u8], length: &mut usize) -> bool {
    let mut bytes = [0u8; 4];
    let encoded = fold_char(c).encode_utf8(&mut bytes);

    if *length + encoded.len() > buffer.len() {
        return false;
    }

    buffer[*length..*length + encoded.len()].copy_from_slice(encoded.as_bytes());
    *length += encoded.len();
    true
}

/// Appends one ASCII byte.
fn push_byte(byte: u8, buffer: &mut [u8], length: &mut usize) -> bool {
    if *length >= buffer.len() {
        return false;
    }

    buffer[*length] = byte;
    *length += 1;
    true
}

/// The rest of `path` after `root`, when `path` starts with `root` character by character under `fold_char`. The rest is
/// `None` when they differ or `path` is shorter. The rest may start inside a name (`C:\Ga` does not contain `C:\Gar`
/// as a folder); the caller checks for the separator. The comparison never splits a character.
pub fn strip_folded_prefix<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    // ASCII is the common path spelling and needs no per-character decoding or folding.
    if path.is_ascii() && root.is_ascii() {
        if path.len() < root.len()
            || !path.as_bytes()[..root.len()].eq_ignore_ascii_case(root.as_bytes())
        {
            return None;
        }

        return Some(&path[root.len()..]);
    }

    let mut path_chars = path.char_indices();
    let mut end = 0;

    for root_char in root.chars() {
        let (at, path_char) = path_chars.next()?;

        if fold_char(root_char) != fold_char(path_char) {
            return None;
        }

        end = at + path_char.len_utf8();
    }

    Some(&path[end..])
}

/// Whether two names are the same name under the folding rule, as Windows compares names: a case change of one name is
/// the same name. Allocates nothing.
pub fn same_folded(left: &str, right: &str) -> bool {
    if left.is_ascii() && right.is_ascii() {
        return left.eq_ignore_ascii_case(right);
    }

    left.chars().map(fold_char).eq(right.chars().map(fold_char))
}

/// `path` relative to `root`, when `path` is `root` or lies inside it. Both use `\` or `/`; the case is ignored, and
/// `.` and `..` are resolved first, so `root\Mods\..\bin` is inside `root`. A sibling with the same start
/// (`C:\Game Mods` for `C:\Game`) is not inside.
pub fn relative_to(root: &str, path: &str) -> Option<String> {
    match relative_direct(root, path) {
        Direct::Inside(rel) => Some(rel),
        Direct::Outside => None,
        Direct::Fallback => {
            // A canonical root needs no normalization; a drive-absolute path can be normalized in one pass without the
            // `replace`, `Vec` and second `normal` of the general fallback.
            if is_canonical_drive(root)
                && let Some(path) = normalize_drive(path)
            {
                return relative_from_normal(root, &path);
            }

            relative_to_normalized(root, path)
        }
    }
}

/// What `strip_folded_prefix` left of a path inside the root: `""` for the root itself, the text after one `\` for a
/// child, and `None` when the two only share a name prefix (`C:\Ga` is not inside `C:\Gar`). Every spelling of the
/// inside/outside boundary goes through this one rule.
fn inside_after_prefix(rest: &str) -> Option<String> {
    if rest.is_empty() {
        Some(String::new())
    } else {
        rest.strip_prefix('\\').map(str::to_string)
    }
}

/// `path` relative to a root that is already canonical (`C:\...`, no `.`/`..`, no trailing separator).
fn relative_from_normal(root: &str, path: &str) -> Option<String> {
    inside_after_prefix(strip_folded_prefix(path, root)?)
}

/// A drive-absolute path in the form `normal` gives it, without the `replace('/', "\\")`, the component `Vec` and the
/// second pass of that function. `None` for anything but `X:\...` (UNC, verbatim, relative and drive-relative paths
/// keep the general code), for a path deeper than the small stack, and for a `/` in the drive prefix.
fn normalize_drive(path: &str) -> Option<String> {
    let bytes = path.as_bytes();

    if bytes.len() < 4 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return None;
    }

    let mut out = String::with_capacity(path.len());
    out.push_str(&path[..3]);
    // The length of `out` before each component; `..` truncates back to it.
    let mut starts = [0usize; 32];
    let mut depth = 0;
    let mut part_start = 3;
    let mut index = 3;

    loop {
        let at_end = index == path.len();
        let separator = at_end || bytes[index] == b'\\' || bytes[index] == b'/';

        if separator {
            match &path[part_start..index] {
                "" | "." => {}
                ".." => {
                    if depth > 0 {
                        depth -= 1;
                        out.truncate(starts[depth]);
                    }
                }
                part => {
                    if depth == starts.len() {
                        return None;
                    }

                    starts[depth] = out.len();

                    if out.len() > 3 {
                        out.push('\\');
                    }

                    out.push_str(part);
                    depth += 1;
                }
            }

            if at_end {
                break;
            }

            part_start = index + 1;
        }

        index += 1;
    }

    Some(out)
}

/// The answer of the no-allocation fast path. `Fallback` means the spelling needs the normalizing code.
enum Direct {
    Inside(String),
    Outside,
    Fallback,
}

/// `relative_to` for two already canonical drive-absolute paths. The common game path needs no `normal` on either side
/// and no allocation when it is outside the root.
fn relative_direct(root: &str, path: &str) -> Direct {
    if !is_canonical_drive(root) || !is_canonical_drive(path) {
        return Direct::Fallback;
    }

    match strip_folded_prefix(path, root).and_then(inside_after_prefix) {
        Some(rel) => Direct::Inside(rel),
        None => Direct::Outside,
    }
}

/// A drive-absolute path with no `/`, no `.`/`..`/empty component and no trailing separator. That is exactly the form
/// `normal` leaves unchanged.
fn is_canonical_drive(text: &str) -> bool {
    let bytes = text.as_bytes();

    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || text.contains('/')
    {
        return false;
    }

    text[3..]
        .split('\\')
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// The old normalizing path, kept for UNC, verbatim, `/`, `.`/`..`, trailing separators and relative roots.
fn relative_to_normalized(root: &str, path: &str) -> Option<String> {
    inside_after_prefix(strip_folded_prefix(&normal(path), &normal(root))?)
}

/// Joins a `\`-separated relative path onto a base folder, one component at a time. The buffer is reserved once, so a
/// path with several components does not grow the `PathBuf` once per component.
pub fn join(base: &Path, rel: &str) -> PathBuf {
    if rel.is_empty() {
        return base.to_path_buf();
    }

    if !rel.contains('\\') {
        return base.join(rel);
    }

    let mut path = PathBuf::with_capacity(base.as_os_str().len() + rel.len() + 2);
    path.push(base);

    for part in rel.split('\\').filter(|part| !part.is_empty()) {
        path.push(part);
    }

    path
}

/// The relative path of the entry `name` inside the folder `rel`. The game folder is `""`.
pub fn join_rel(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_string()
    } else {
        let mut path = String::with_capacity(rel.len() + 1 + name.len());
        path.push_str(rel);
        path.push('\\');
        path.push_str(name);
        path
    }
}

/// The FNV-1a offset basis and prime. The hash only answers a fast negative; a collision still goes through the exact
/// `BTreeSet` lookup, so a hash collision cannot change the result.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// One FNV-1a step over a byte that is already folded. `key_hash` and the incremental prefix hashes of `hides` and
/// `hides_folded` all mix through this one step, so they cannot drift apart.
#[inline]
fn fnv_step(hash: u64, folded: u8) -> u64 {
    (hash ^ u64::from(folded)).wrapping_mul(FNV_PRIME)
}

/// FNV-1a over the bytes with ASCII case folded.
fn key_hash(key: &str) -> u64 {
    let mut hash = FNV_OFFSET;

    for byte in key.bytes() {
        hash = fnv_step(hash, byte.to_ascii_lowercase());
    }

    hash
}

/// Matches a file name against a Windows search pattern: `*` is any text, `?` is one character, and the case is
/// ignored. `*.*` matches every name, as it does in `FindFirstFile`.
pub fn wildcard_match(pattern: &str, name: &str) -> bool {
    if pattern == "*.*" {
        return true;
    }
    // A pattern that ends in a dot with no other dot before it means "no extension": `*.` lists the names without a
    // dot, and `.` and `..` match when the rest of the pattern does. A name with a dot is not in that list.
    if let Some(stem) = pattern.strip_suffix('.')
        && !stem.contains('.')
    {
        if name == "." || name == ".." {
            return stem.chars().all(|c| c == '*');
        }

        return !name.contains('.') && wildcard_match(stem, name);
    }

    if pattern.is_ascii() && name.is_ascii() {
        return matches_from(pattern.as_bytes(), name.as_bytes());
    }

    let pattern: Vec<char> = pattern.chars().map(fold_char).collect();
    let name: Vec<char> = name.chars().map(fold_char).collect();
    matches_from(&pattern, &name)
}

/// `wildcard_match` for a UTF-16 name, as a directory listing returns it. An ASCII name is matched in a stack buffer
/// without a `String`; any other name keeps the text rule.
pub fn wildcard_match_units(pattern: &str, name: &[u16]) -> bool {
    // The listing mask is usually `*`; no name needs to be converted or matched.
    if pattern == "*" || pattern == "*.*" {
        return true;
    }

    if pattern.is_ascii() {
        let mut ascii = [0u8; 260];
        let length = name.len().min(ascii.len());

        if length == name.len() {
            let mut all_ascii = true;

            for (slot, unit) in ascii.iter_mut().zip(name) {
                if *unit >= 0x80 {
                    all_ascii = false;
                    break;
                }

                *slot = *unit as u8;
            }

            if all_ascii && let Ok(name) = std::str::from_utf8(&ascii[..length]) {
                return wildcard_match(pattern, name);
            }
        }
    }

    wildcard_match(pattern, &String::from_utf16_lossy(name))
}

/// One element of a wildcard pattern and a name: a byte or a character.
trait MatchUnit {
    fn is_star(&self) -> bool;
    fn is_question(&self) -> bool;
    fn matches(self, other: Self) -> bool;
}

impl MatchUnit for u8 {
    fn is_star(&self) -> bool {
        *self == b'*'
    }

    fn is_question(&self) -> bool {
        *self == b'?'
    }

    fn matches(self, other: Self) -> bool {
        self.eq_ignore_ascii_case(&other)
    }
}

impl MatchUnit for char {
    fn is_star(&self) -> bool {
        *self == '*'
    }

    fn is_question(&self) -> bool {
        *self == '?'
    }

    fn matches(self, other: Self) -> bool {
        self == other
    }
}

/// Matches `name` against `pattern` in one pass with backtracking to the last `*`, so the time is quadratic at worst
/// instead of exponential. A `*` that matched too much is widened by one character each time the match fails.
fn matches_from<T: MatchUnit + Copy>(pattern: &[T], name: &[T]) -> bool {
    let (mut p, mut n) = (0, 0);
    // The pattern index after the last `*`, and the name index that `*` has matched up to.
    let mut star: Option<(usize, usize)> = None;

    while n < name.len() {
        match pattern.get(p) {
            Some(&unit) if unit.is_star() => {
                star = Some((p + 1, n));
                p += 1;
            }
            Some(&unit) if unit.is_question() => {
                p += 1;
                n += 1;
            }
            Some(&wanted) if wanted.matches(name[n]) => {
                p += 1;
                n += 1;
            }
            _ => match star {
                Some((resume, matched)) => {
                    star = Some((resume, matched + 1));
                    p = resume;
                    n = matched + 1;
                }
                None => return false,
            },
        }
    }

    pattern[p..].iter().all(|&rest| rest.is_star())
}

/// A path as text in Windows form: `\` separators, no `\\?\` prefix, no trailing separator, and `.` and `..`
/// resolved. The case is kept. `..` never climbs above the drive (`C:`) or the server (`\\server`) of the path.
fn normal(path: &str) -> String {
    let text = path.replace('/', "\\");
    let text = text.strip_prefix("\\\\?\\").unwrap_or(&text);
    // `\\?\UNC\server\share` is the same folder as `\\server\share`.
    let unc = text
        .get(..4)
        .filter(|head| head.eq_ignore_ascii_case("UNC\\"));
    let text = match unc {
        Some(_) => format!("\\\\{}", &text[4..]),
        None => text.to_string(),
    };
    let text = text.as_str();
    let (root, rest) = if let Some(rest) = text.strip_prefix("\\\\") {
        ("\\\\", rest)
    } else if let [drive, b':', ..] = text.as_bytes()
        && drive.is_ascii_alphabetic()
    {
        text.split_at(2)
    } else {
        ("", text)
    };
    let mut parts: Vec<&str> = Vec::new();

    for part in rest.split('\\') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }

    let mut out = root.to_string();

    for part in parts {
        if !out.is_empty() && !out.ends_with('\\') {
            out.push('\\');
        }

        out.push_str(part);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers() -> Layers {
        Layers {
            root: PathBuf::from("/game"),
            staging: Some(PathBuf::from("/state/staging")),
            upper: PathBuf::from("/state/upper"),
        }
    }

    /// An absolute path in this platform's spelling, so a config built for a test passes validation.
    fn absolute(rel: &str) -> PathBuf {
        #[cfg(windows)]
        let path = format!(r"C:\{rel}");
        #[cfg(not(windows))]
        let path = format!("/{rel}");
        PathBuf::from(path)
    }

    fn mount(id: &str) -> MountSpec {
        MountSpec {
            id: id.to_string(),
            guest: absolute("game"),
            root: absolute("game"),
            staging: None,
            upper: absolute("state").join("upper"),
        }
    }

    #[test]
    fn relative_paths_inside_the_game_folder_ignore_case_and_separators() {
        assert_eq!(
            relative_to(r"C:\Games\Example", r"c:\games\example\Mods\a.dat"),
            Some(r"Mods\a.dat".to_string())
        );
        assert_eq!(
            relative_to(r"C:\Games\Example", "C:/Games/Example/Mods/"),
            Some("Mods".to_string())
        );
        assert_eq!(
            relative_to(r"C:\Games\Example", r"\\?\C:\Games\Example"),
            Some(String::new())
        );
    }

    #[test]
    fn a_verbatim_unc_path_is_the_same_folder_as_the_unc_path() {
        assert_eq!(
            relative_to(
                r"\\server\share\Game",
                r"\\?\UNC\server\share\Game\bin\x.dll"
            ),
            Some(r"bin\x.dll".to_string())
        );
        assert_eq!(
            relative_to(r"\\server\share\Game", r"\\?\UNC\server\share\Other\x"),
            None
        );
    }

    #[test]
    fn dot_and_dot_dot_are_resolved_before_the_comparison() {
        // A write through `..` must reach the same layer as the plain path, or it would land in the game folder.
        assert_eq!(
            relative_to(r"C:\Game", r"C:\Game\Mods\..\bin\x.dll"),
            Some(r"bin\x.dll".to_string())
        );
        assert_eq!(
            relative_to(r"C:\Game", r"C:\Game\.\bin\.\x.dll"),
            Some(r"bin\x.dll".to_string())
        );
        assert_eq!(relative_to(r"C:\Game", r"C:\Game\..\Other\x"), None);
        assert_eq!(
            relative_to(r"C:\Game", r"C:/Game//bin/x.dll"),
            Some(r"bin\x.dll".to_string())
        );
    }

    #[test]
    fn a_dotted_drive_path_is_normalized_in_one_pass() {
        assert_eq!(
            relative_to(r"C:\Game", r"C:\Game\mods\..\content\.\item.xml"),
            Some(r"content\item.xml".to_string())
        );
        assert_eq!(
            relative_to(r"C:\Game", r"C:\Game\\bin/x.dll"),
            Some(r"bin\x.dll".to_string())
        );
        assert_eq!(relative_to(r"C:\Game", r"C:\Game\"), Some(String::new()));
        assert_eq!(
            relative_to(r"C:\Game", r"C:\Game\..\..\Game\x"),
            Some("x".to_string())
        );
    }

    #[test]
    fn dot_dot_does_not_climb_above_the_drive() {
        assert_eq!(
            relative_to(r"C:\Game", r"C:\..\..\Game\x"),
            Some(r"x".to_string())
        );
    }

    #[test]
    fn reads_see_the_top_most_layer_that_has_the_file() {
        let layers = layers();
        let present = [
            PathBuf::from("/state/upper/data/a.txt"),
            PathBuf::from("/state/staging/data/a.txt"),
            PathBuf::from("/state/staging/data/b.txt"),
            PathBuf::from("/game/data/b.txt"),
            PathBuf::from("/game/data/c.txt"),
        ];
        let exists = |path: &Path| present.iter().any(|known| known == path);

        assert_eq!(
            layers.resolve("data\\a.txt", exists),
            Some((Layer::Upper, PathBuf::from("/state/upper/data/a.txt")))
        );
        assert_eq!(
            layers.resolve("data\\b.txt", exists),
            Some((Layer::Staging, PathBuf::from("/state/staging/data/b.txt")))
        );
        assert_eq!(
            layers.resolve("data\\c.txt", exists),
            Some((Layer::Root, PathBuf::from("/game/data/c.txt")))
        );
        assert_eq!(layers.resolve("data\\d.txt", exists), None);
    }

    #[test]
    fn writes_go_to_the_upper_layer_only() {
        assert_eq!(
            layers().write_target("data\\new.txt"),
            PathBuf::from("/state/upper/data/new.txt")
        );
    }

    #[test]
    fn search_patterns_match_like_windows() {
        assert!(wildcard_match("*", "anything.pak"));
        assert!(wildcard_match("*.*", "no-extension"));
        assert!(wildcard_match("*.PAK", "mod.pak"));
        assert!(!wildcard_match("*.pak", "mod.pak.bak"));
        assert!(wildcard_match("a?c", "ABC"));
        assert!(!wildcard_match("a?c", "ac"));
        assert!(wildcard_match("mod*0", "mod0000_MergedFiles.0"));
    }

    #[test]
    fn a_star_gives_back_what_it_matched_when_the_rest_fails() {
        assert!(wildcard_match("*a*b", "xaxab"));
        assert!(wildcard_match("a*b*c", "abxbc"));
        assert!(!wildcard_match("a*b*c", "acb"));
        assert!(wildcard_match("*a", "AA"));
        assert!(wildcard_match("*", ""));
        assert!(!wildcard_match("*?", ""));
        assert!(!wildcard_match("??", "a"));
        assert!(!wildcard_match("*.pak", "mod.pak.bak"));
        assert!(wildcard_match("*.pak", "mod.PAK"));
    }

    #[test]
    fn a_utf16_name_matches_like_its_text() {
        for (pattern, name) in [
            ("*", "a"),
            ("*.PAK", "mod.pak"),
            ("a?c", "ABC"),
            ("*.", "content0"),
            ("*", "świat.txt"),
            ("*.TXT", "żółć.txt"),
        ] {
            let units: Vec<u16> = name.encode_utf16().collect();
            assert_eq!(
                wildcard_match_units(pattern, &units),
                wildcard_match(pattern, name),
                "{pattern} / {name}"
            );
        }
    }

    #[test]
    fn a_pattern_of_many_stars_matches_in_polynomial_time() {
        // Every `*` tried over every character: the old recursive matcher took exponential time on this.
        let pattern = "*a".repeat(40) + "b";
        let name = "a".repeat(400);
        assert!(!wildcard_match(&pattern, &name));
    }

    #[test]
    fn a_trailing_dot_lists_the_names_without_an_extension() {
        // The game lists its folders with `*.`: the folders are the names without a dot, and `.` and `..` come too.
        assert!(wildcard_match("*.", "content0"));
        assert!(wildcard_match("*.", "."));
        assert!(wildcard_match("*.", ".."));
        assert!(!wildcard_match("*.", "metadata.store"));
        assert!(!wildcard_match("*.", "content0.bak"));
        assert!(wildcard_match("c*.", "content0"));
        assert!(!wildcard_match("c*.", "dlc"));
    }

    #[test]
    fn the_layers_come_from_all_three_variables_or_not_at_all() {
        let full = |name: &str| match name {
            ENV_ROOT => Some("/game".to_string()),
            ENV_STAGING => Some("/state/staging".to_string()),
            ENV_UPPER => Some("/state/upper".to_string()),
            _ => None,
        };
        assert_eq!(Layers::from_env(full), Some(layers()));
        assert_eq!(
            Layers::from_env(|name| if name == ENV_ROOT {
                Some(String::new())
            } else {
                full(name)
            }),
            None
        );
        assert_eq!(Layers::from_env(|_| None), None);
    }

    #[test]
    fn the_environment_round_trips() {
        let layers = layers();
        let env = layers.to_env();
        let back = Layers::from_env(|name| {
            env.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
        });
        assert_eq!(back, Some(layers));
    }

    #[test]
    fn a_whiteout_hides_the_lower_layers_but_not_the_upper_one() {
        let layers = layers();
        // The game deleted `data\a.txt`: the staging and the game copies are hidden, a new upper copy is not.
        let present = [
            PathBuf::from("/state/staging/data/a.txt"),
            PathBuf::from("/game/data/a.txt"),
        ];
        let exists = |path: &Path| present.iter().any(|known| known == path);
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide("Data\\A.txt");

        assert_eq!(
            layers.resolve_visible("data\\a.txt", exists, &whiteouts),
            None
        );
        let upper_copy = |path: &Path| path == Path::new("/state/upper/data/a.txt") || exists(path);
        assert_eq!(
            layers.resolve_visible("data\\a.txt", upper_copy, &whiteouts),
            Some((Layer::Upper, PathBuf::from("/state/upper/data/a.txt")))
        );
    }

    #[test]
    fn a_whiteout_on_a_folder_hides_everything_below_it() {
        let layers = layers();
        let present = [PathBuf::from("/game/archive/pc/mod/b.archive")];
        let exists = |path: &Path| present.iter().any(|known| known == path);
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide("archive");

        assert!(whiteouts.hides("archive\\pc\\mod\\b.archive"));
        assert!(!whiteouts.hides("archives\\x"));
        assert!(!whiteouts.hides(""));
        assert_eq!(
            layers.resolve_visible("archive\\pc\\mod\\b.archive", exists, &whiteouts),
            None
        );
        let kept: Vec<&str> = ["archive", "bin", "Archive2"]
            .into_iter()
            .filter(|name| !whiteouts.hides_child_in_visible_folder("", name))
            .collect();
        assert_eq!(kept, ["bin", "Archive2"]);
    }

    #[test]
    fn a_whiteout_with_a_letter_outside_ascii_is_folded_without_a_string() {
        let whiteouts = Whiteouts::parse("Ś\\save.dat");
        assert!(whiteouts.hides("ś\\save.dat"));
        assert!(whiteouts.hides("Ś\\SAVE.DAT"));
        assert!(whiteouts.hides_child_in_visible_folder("Ś", "SAVE.DAT"));
        let name: Vec<u16> = "SAVE.DAT".encode_utf16().collect();
        assert!(whiteouts.hides_child_units_in_visible_folder("Ś", &name));
        let path: Vec<u16> = "ś\\save.dat".encode_utf16().collect();
        assert!(whiteouts.hides(&String::from_utf16(&path).unwrap()));
    }

    #[test]
    fn whiteouts_survive_their_file_format() {
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide(r"bin\x64\Game.exe");
        whiteouts.hide("archive");
        let text = whiteouts.to_text();
        assert_eq!(text, "archive\nbin\\x64\\game.exe\n");
        assert_eq!(Whiteouts::parse(&text), whiteouts);
        assert_eq!(Whiteouts::parse("\n  \n"), Whiteouts::default());
    }

    #[test]
    fn the_hidden_paths_are_listed_for_a_caller_that_moves_them_back() {
        let whiteouts = Whiteouts::parse("archive\nbin\\x64\\game.exe\n");
        assert_eq!(
            whiteouts.iter().collect::<Vec<_>>(),
            ["archive", "bin\\x64\\game.exe"]
        );
        assert_eq!(Whiteouts::default().iter().count(), 0);
    }

    #[test]
    fn a_name_inside_a_folder_is_joined_with_a_backslash() {
        assert_eq!(join_rel("", "a.txt"), "a.txt");
        assert_eq!(join_rel("Mods", "a.txt"), "Mods\\a.txt");
    }

    #[test]
    fn only_a_folder_or_an_immediate_child_whiteout_affects_its_listing() {
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide(r"archive\pc\mod\a.bin");
        assert!(!whiteouts.affects_listing("archive"));
        assert!(!whiteouts.affects_listing(r"archive\pc"));
        assert!(whiteouts.affects_listing(r"archive\pc\mod"));
        assert!(!whiteouts.affects_listing("other"));

        whiteouts.hide(r"archive\pc");
        assert!(whiteouts.affects_listing("archive"));
        whiteouts.hide("archive");
        assert!(whiteouts.affects_listing(""));
    }

    #[test]
    fn a_whiteout_child_check_matches_the_full_path_check() {
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide("archive");
        whiteouts.hide(r"data\A.txt");
        assert!(whiteouts.hides_child_in_visible_folder("data", "a.txt"));
        assert!(whiteouts.hides_child_units_in_visible_folder(
            "data",
            &"A.TXT".encode_utf16().collect::<Vec<_>>()
        ));
        // A hidden folder hides everything below it, so the child check is never reached for it.
        assert!(whiteouts.hides("archive"));
        assert!(!whiteouts.hides_child_in_visible_folder("data", "b.txt"));
        assert!(
            !whiteouts.hides_child_units_in_visible_folder(
                "other",
                &"x".encode_utf16().collect::<Vec<_>>()
            )
        );
    }

    #[test]
    fn the_ascii_fast_path_keeps_the_unicode_fallback() {
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide(r"data\Ä.txt");
        assert!(whiteouts.hides(r"DATA\Ä.txt"));
        assert!(whiteouts.hides(r"data\ä.txt"));
        assert!(!whiteouts.hides(r"data\a.txt"));
    }

    fn reference_hides(paths: &BTreeSet<String>, rel: &str) -> bool {
        if rel.is_empty() || paths.is_empty() {
            return false;
        }

        let lower = rel.to_lowercase();
        lower
            .match_indices('\\')
            .map(|(at, _)| &lower[..at])
            .chain(std::iter::once(lower.as_str()))
            .any(|path| paths.contains(path))
    }

    /// The fast whiteout check and the reference agree for these hidden paths.
    fn agrees_with_reference(hidden: &[String], rel: &str) -> bool {
        let mut whiteouts = Whiteouts::default();

        for path in hidden {
            whiteouts.hide(path);
        }

        whiteouts.hides(rel) == reference_hides(&whiteouts.paths, rel)
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn the_fast_whiteout_check_is_the_same_as_the_reference(
            hidden in prop::collection::vec("[a-zA-Z0-9_\\\\.]{0,30}", 0..8),
            rel in "[a-zA-Z0-9_\\\\.]{0,40}",
        ) {
            prop_assert!(agrees_with_reference(&hidden, &rel));
        }

        #[test]
        fn a_hidden_folder_hides_everything_below_it(
            folder in "[a-zA-Z0-9_\\\\]{1,30}",
            child in "[a-zA-Z0-9_\\\\]{1,30}",
        ) {
            let rel = format!("{folder}\\{child}");
            let mut whiteouts = Whiteouts::default();
            whiteouts.hide(&folder);
            prop_assert!(whiteouts.hides(&rel));
            prop_assert!(agrees_with_reference(std::slice::from_ref(&folder), &rel));
        }

        #[test]
        fn the_folded_whiteout_check_is_the_same_as_the_reference(
            hidden in prop::collection::vec("[a-zA-Z0-9_\\\\.śŚżŹ]{0,20}", 0..6),
            rel in "[a-zA-Z0-9_\\\\.śŚżŹ]{0,30}",
        ) {
            prop_assert!(agrees_with_reference(&hidden, &rel));
        }
    }

    #[test]
    fn relative_to_returns_game_relative_paths_for_separators_and_unc() {
        // A canonical root, a path with separators and dot components, and a UNC root take different paths through
        // `relative_to`; the answer must be the same kind of game-relative path on each.
        for (root, path, expected) in [
            (r"C:\Game", r"C:\Game\data\a.txt", Some(r"data\a.txt")),
            (
                r"C:\Game",
                r"c:/game/mods/../data/a.txt",
                Some(r"data\a.txt"),
            ),
            (r"C:\Game", r"C:\Game Mods\data\a.txt", None),
            (
                r"\\server\share\Game",
                r"\\?\UNC\server\share\Game\data\a.txt",
                Some(r"data\a.txt"),
            ),
        ] {
            assert_eq!(
                relative_to(root, path),
                expected.map(str::to_string),
                "{path}"
            );
        }
    }

    proptest! {
        #[test]
        fn a_path_below_the_root_is_relative(
            root in "[a-z]{1,8}",
            parts in prop::collection::vec("[a-z]{1,8}", 1..5),
        ) {
            let base = format!(r"C:\{root}");
            let relative = parts.join("\\");
            let path = format!("{base}\\{relative}");
            prop_assert_eq!(relative_to(&base, &path), Some(relative));
        }

        #[test]
        fn a_sibling_with_the_same_prefix_is_outside(
            root in "[a-z]{1,8}",
            extra in "[a-z]{1,8}",
        ) {
            let base = format!(r"C:\{root}");
            let path = format!(r"C:\{root}{extra}\a.txt");
            prop_assert_eq!(relative_to(&base, &path), None);
        }
    }

    #[test]
    fn the_game_mount_and_the_redirects_follow_the_config_shape() {
        let config = MountConfig::try_new(
            MountConfig::VERSION,
            vec![mount(MountSpec::GAME_ID), mount("saves")],
        )
        .unwrap();
        assert_eq!(config.game().map(|mount| mount.id.as_str()), Some("game"));
        assert_eq!(
            config
                .redirects()
                .map(|mount| mount.id.as_str())
                .collect::<Vec<_>>(),
            ["saves"]
        );
        // `game` still says `None` for a set assembled without validation.
        let mut wrong_first = config.clone();
        wrong_first.mounts = vec![mount("saves")];
        assert_eq!(wrong_first.game(), None);
        let mut empty = config;
        empty.mounts = Vec::new();
        assert_eq!(empty.game(), None);
    }

    #[test]
    fn a_valid_two_mount_config_is_accepted() {
        let config = MountConfig::try_new(
            MountConfig::VERSION,
            vec![mount(MountSpec::GAME_ID), mount("saves")],
        )
        .unwrap();
        assert_eq!(config.mounts().len(), 2);
        assert_eq!(config.mounts()[0].id, MountSpec::GAME_ID);
        assert_eq!(config.redirects().count(), 1);
        assert!(config.clone().validate().is_ok());
    }

    #[test]
    fn too_many_mounts_are_refused() {
        let mut mounts = vec![mount(MountSpec::GAME_ID)];
        mounts.extend((1..=MAX_MOUNTS).map(|index| mount(&format!("set-{index}"))));
        assert_eq!(mounts.len(), MAX_MOUNTS + 1);
        assert!(MountConfig::try_new(MountConfig::VERSION, mounts.clone()).is_err());
        mounts.truncate(MAX_MOUNTS);
        assert!(MountConfig::try_new(MountConfig::VERSION, mounts).is_ok());
    }

    #[test]
    fn a_config_without_the_game_mount_first_is_refused() {
        assert!(
            MountConfig::try_new(
                MountConfig::VERSION,
                vec![mount("saves"), mount(MountSpec::GAME_ID)],
            )
            .is_err()
        );
        assert!(MountConfig::try_new(MountConfig::VERSION, Vec::new()).is_err());
    }

    #[test]
    fn duplicate_and_empty_mount_ids_are_refused() {
        assert!(
            MountConfig::try_new(
                MountConfig::VERSION,
                vec![mount(MountSpec::GAME_ID), mount(MountSpec::GAME_ID)],
            )
            .is_err()
        );
        assert!(
            MountConfig::try_new(
                MountConfig::VERSION,
                vec![mount(MountSpec::GAME_ID), mount("")],
            )
            .is_err()
        );
    }

    #[test]
    fn a_relative_mount_path_is_refused() {
        let config = MountConfig::try_new(
            MountConfig::VERSION,
            vec![mount(MountSpec::GAME_ID), mount("saves")],
        )
        .unwrap();
        let mut relative_guest = config.clone();
        relative_guest.mounts[0].guest = PathBuf::from("relative");
        assert!(relative_guest.validate().is_err());
        let mut relative_root = config.clone();
        relative_root.mounts[0].root = PathBuf::from("relative");
        assert!(relative_root.validate().is_err());
        let mut relative_upper = config.clone();
        relative_upper.mounts[0].upper = PathBuf::from("relative");
        assert!(relative_upper.validate().is_err());
        let mut relative_staging = config;
        relative_staging.mounts[0].staging = Some(PathBuf::from("relative"));
        assert!(relative_staging.validate().is_err());
    }

    #[test]
    fn the_layer_helpers_cover_the_paths_and_the_missing_staging() {
        let layers = layers();
        assert_eq!(
            layers
                .layer_paths()
                .map(|(layer, _)| layer)
                .collect::<Vec<_>>(),
            [Layer::Upper, Layer::Staging, Layer::Root]
        );
        let no_staging = Layers::new(PathBuf::from("/game"), None, PathBuf::from("/state/upper"));
        assert_eq!(
            no_staging
                .layer_paths()
                .map(|(layer, _)| layer)
                .collect::<Vec<_>>(),
            [Layer::Upper, Layer::Root]
        );
        assert_eq!(
            layers.whiteouts_file(),
            PathBuf::from("/state/upper.whiteouts")
        );
        assert_eq!(
            layers.whiteouts_journal_file(),
            PathBuf::from("/state/upper.whiteouts.journal")
        );
    }

    #[test]
    fn a_whiteout_set_is_empty_until_a_path_is_hidden() {
        let mut whiteouts = Whiteouts::default();
        assert!(whiteouts.is_empty());
        whiteouts.hide("data\\a.txt");
        assert!(!whiteouts.is_empty());
        assert!(!Whiteouts::parse("data\\a.txt").is_empty());
    }

    #[test]
    fn same_folded_compares_names_as_windows_does() {
        assert!(same_folded("Ś", "ś"));
        assert!(same_folded("Data\\A.TXT", "data\\a.txt"));
        assert!(!same_folded("a", "b"));
        assert!(!same_folded("a", "aa"));
    }

    #[test]
    fn only_canonical_drive_paths_take_the_direct_route() {
        assert!(is_canonical_drive(r"C:\Game"));
        assert!(is_canonical_drive(r"C:\Game\bin"));
        assert!(is_canonical_drive(r"C:\A"));
        assert!(!is_canonical_drive(""));
        assert!(!is_canonical_drive("C:"));
        assert!(!is_canonical_drive(r"C:\"));
        assert!(!is_canonical_drive(r"C:\Game\"));
        assert!(!is_canonical_drive(r"C:/Game"));
        assert!(!is_canonical_drive(r"C:\Game/"));
        assert!(!is_canonical_drive(r"C?\Game"));
        assert!(!is_canonical_drive(r"C\Game"));
        assert!(!is_canonical_drive(r"C:\Game\.."));
        assert!(!is_canonical_drive(r"1:\Game"));
        assert!(!is_canonical_drive(r"C:\Game\\bin"));
    }

    #[test]
    fn normal_keeps_one_separator_after_a_root() {
        assert_eq!(normal(r"\\server\share"), r"\\server\share");
        assert_eq!(normal(r"Game\bin"), r"Game\bin");
        assert_eq!(normal(r"C:\Game\..\Other"), r"C:\Other");
        assert_eq!(normal(r"C:/Game//bin/"), r"C:\Game\bin");
    }

    #[test]
    fn a_non_ascii_wildcard_uses_the_character_matcher() {
        assert!(wildcard_match("Ś*.TXT", "śfile.txt"));
        assert!(wildcard_match("ś?file", "ŚXFILE"));
        assert!(wildcard_match("?", "ś"));
    }

    #[test]
    fn the_utf16_ascii_check_accepts_7f_and_rejects_80() {
        assert!(units_are_ascii(&[]));
        assert!(units_are_ascii(&[0x41, 0x7F]));
        assert!(!units_are_ascii(&[0x80]));
        assert!(!units_are_ascii(&[0x41, 0x80]));
    }

    #[test]
    fn folding_into_a_non_ascii_buffer_checks_the_exact_size() {
        let mut exact = [0u8; 2];
        assert_eq!(fold_key_into("ś", &mut exact), Some("ś"));
        let mut short = [0u8; 1];
        assert_eq!(fold_key_into("ś", &mut short), None);
    }

    #[test]
    fn a_fold_into_an_exact_buffer_fits_and_a_short_one_is_none() {
        let mut exact = [0u8; 3];
        assert_eq!(fold_key_into("ABC", &mut exact), Some("abc"));
        let mut short = [0u8; 2];
        assert_eq!(fold_key_into("ABC", &mut short), None);
    }

    #[test]
    fn a_utf16_name_longer_than_the_stack_buffer_uses_the_lossy_path() {
        // The folded name must exceed the 1024-byte stack buffer, so the fold fails and the lossy `String` path runs.
        let prefix = "a".repeat(1100);
        let mut whiteouts = Whiteouts::default();
        whiteouts.hide(&format!("data\\{prefix}"));
        let name: Vec<u16> = format!("{prefix}{}", "z".repeat(40))
            .encode_utf16()
            .collect();
        assert!(!whiteouts.hides_child_units_in_visible_folder("data", &name));
    }
}

#[cfg(test)]
mod fold_tests;
