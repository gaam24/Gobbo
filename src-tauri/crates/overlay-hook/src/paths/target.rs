//! The file a path means for a call: the upper copy for a write, the top-most visible layer for a read, and the copy-up
//! that keeps a write away from the game folder. Also the access masks that ask for a write.

use rustc_hash::FxHashMap;
use std::fs::{self, FileTimes};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use gobbo_overlay_core::{Layer, copy_temp_name, join, wide};
use gobbo_overlay_decisions::{Kind, Layer as DecisionLayer};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_DIRECTORY, ERROR_DISK_FULL, ERROR_FILE_EXISTS,
    ERROR_FILE_NOT_FOUND, ERROR_GEN_FAILURE, ERROR_PATH_NOT_FOUND, GENERIC_ALL, GENERIC_WRITE,
    STATUS_ACCESS_DENIED, STATUS_DISK_FULL, STATUS_NOT_A_DIRECTORY, STATUS_OBJECT_NAME_COLLISION,
    STATUS_OBJECT_NAME_NOT_FOUND, STATUS_OBJECT_PATH_NOT_FOUND, STATUS_UNSUCCESSFUL,
};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_NEW, DELETE, FILE_APPEND_DATA, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA,
    WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::SystemServices::{ACCESS_SYSTEM_SECURITY, MAXIMUM_ALLOWED};

use super::disk::{exists, make_parent};
use super::parent_rel;
use super::probe::{Missing, Probe, probe_child};
use crate::diag::{decision_layer, record_decision};
use crate::state::Mount;
use crate::trace::trace;
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY;

/// Whether an access mask asks to change the file, or to get any right that can: write, append, delete, change its
/// attributes or its security, or the maximum of the rights the file allows (which includes writing). Such a file is
/// opened in the upper layer, so the layer below it is never touched.
pub(crate) fn writes_requested(access: u32) -> bool {
    access
        & (GENERIC_WRITE
            | GENERIC_ALL
            | FILE_WRITE_DATA
            | FILE_APPEND_DATA
            | FILE_WRITE_EA
            | FILE_WRITE_ATTRIBUTES
            | DELETE
            | WRITE_DAC
            | WRITE_OWNER
            | ACCESS_SYSTEM_SECURITY
            | MAXIMUM_ALLOWED)
        != 0
}

/// Why a path cannot be used for a call: no layer shows it, a Windows error from the write path, an access refusal, or
/// an I/O error from the copy-up. The Win32 and NT callers turn this into the error their API reports.
#[derive(Debug)]
pub(crate) enum Failure {
    Missing(Missing),
    Denied(u32),
    Io(io::Error),
}

impl Failure {
    /// The error a Win32 call reports for it.
    pub(crate) fn win32(&self) -> u32 {
        match self {
            Self::Missing(missing) => missing.win32(),
            Self::Denied(code) => *code,
            Self::Io(error) => error
                .raw_os_error()
                .map_or(ERROR_GEN_FAILURE, |code| code as u32),
        }
    }

    /// The status an NT call reports for it.
    pub(crate) fn status(&self) -> i32 {
        match self {
            Self::Missing(missing) => missing.status(),
            Self::Denied(_) | Self::Io(_) => win32_to_status(self.win32()),
        }
    }
}

impl From<Missing> for Failure {
    fn from(missing: Missing) -> Self {
        Self::Missing(missing)
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// The status for the errors a copy-up or a write can report. The NT layer has no general converter in user mode.
fn win32_to_status(code: u32) -> i32 {
    match code {
        ERROR_FILE_NOT_FOUND => STATUS_OBJECT_NAME_NOT_FOUND,
        ERROR_PATH_NOT_FOUND => STATUS_OBJECT_PATH_NOT_FOUND,
        ERROR_ACCESS_DENIED => STATUS_ACCESS_DENIED,
        ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS => STATUS_OBJECT_NAME_COLLISION,
        ERROR_DISK_FULL => STATUS_DISK_FULL,
        ERROR_DIRECTORY => STATUS_NOT_A_DIRECTORY,
        _ => STATUS_UNSUCCESSFUL,
    }
}

/// The name a call of the program passes on. `Same` means the original pointer is already the game path: the top-most
/// layer is the game folder, so re-encoding the name would only allocate.
pub(crate) enum Redirected {
    Same,
    Layer { name: Vec<u16> },
}

/// Where a file the program opens really is. `Same` is a read that stays in the game folder; `Layer` is the path to pass
/// to the original call.
pub(crate) enum Target {
    Same,
    Layer { path: PathBuf },
}

/// The name of a file the program opens, redirected.
///
/// # Safety
/// `name` must be a NUL-terminated UTF-16 string, or null.
pub(crate) unsafe fn redirect_opened(
    name: *const u16,
    writing: bool,
    keeps_contents: bool,
    disposition: u32,
) -> Option<Result<Redirected, Failure>> {
    let place = unsafe { super::place(name) }?;
    Some(
        resolve(
            place.mount,
            &place.rel,
            writing,
            keeps_contents,
            place.mount.guest_is_root(),
            disposition,
        )
        .map(|target| match target {
            Target::Same => Redirected::Same,
            Target::Layer { path } => Redirected::Layer {
                name: wide(path.as_os_str()),
            },
        }),
    )
}

/// The same as `redirect_opened`, for a game-relative path the NT layer has already classified. The NT layer always
/// builds an absolute name (it clears `root_directory`), so a root read is still returned with its path.
/// The NT path a caller already assigned to a mount.
pub(crate) fn redirect_relative_on(
    mount: &Mount,
    rel: &str,
    writing: bool,
    keeps_contents: bool,
) -> Result<Target, Failure> {
    // The NT layer has no Win32 creation disposition; a `CREATE_NEW`-style refusal is its own caller's rule.
    resolve(mount, rel, writing, keeps_contents, false, 0)
}

/// Whether a `CREATE_NEW` open must refuse a name the write probes already showed. Upper and lower answers are passed
/// in separately so this stays a pure decision: a name shown by any layer is taken, and nothing is copied first.
fn create_new_taken(create_new: bool, upper_found: bool, lower_visible: bool) -> bool {
    create_new && (upper_found || lower_visible)
}

/// The file a game-relative path means for a call. A write goes to the upper copy, made first when the program keeps the
/// old contents. A read goes to the top-most layer that shows the file. A read that no layer shows is `Missing`.
/// `same_root` lets a Win32 read keep the caller's pointer when the game folder is the top-most layer. `disposition`
/// carries `CREATE_NEW` into this single resolution, so an existing name is refused without a second classification or
/// a separate visible probe.
fn resolve(
    mount: &Mount,
    rel: &str,
    writing: bool,
    keeps_contents: bool,
    same_root: bool,
    disposition: u32,
) -> Result<Target, Failure> {
    if writing {
        let create_new = disposition == CREATE_NEW;
        // A write that already has an upper copy does not need the logical-parent check: the upper file cannot exist
        // without its folder, and the copy-up has nothing to do. This is the steady state for a game that rewrites its
        // own save or config; it saves one probe and one `create_dir_all` on every such open.
        let upper = probe_child(&mount.upper, rel);
        if create_new_taken(create_new, upper.is_found(), false) {
            return Err(Failure::Denied(ERROR_FILE_EXISTS));
        }
        match upper {
            Probe::Found { .. } => {
                let path = mount.write_target(rel);
                record_decision(Kind::Write, DecisionLayer::Upper, 0, || {
                    mount.decision_path(rel)
                });
                trace(|| format!("write {rel} already upper -> {}", path.display()));
                return Ok(Target::Layer { path });
            }
            Probe::Denied(code) => {
                record_decision(Kind::Refuse, DecisionLayer::None, code, || {
                    mount.decision_path(rel)
                });
                return Err(Failure::Denied(code));
            }
            Probe::Missing(_) => {}
        }
        // The upper probe above is handed to the layer walk, which would otherwise ask the upper layer the same
        // question again. One visible probe answers the copy source, the spelling of the new copy and, when it finds a
        // file, the parent check: a visible file cannot have a hidden or missing parent. A truncating write needs no
        // copy, but its new copy still takes the spelling of a lower file, so the probe runs for it too; a refused probe
        // then does not stop the write, as it did not before.
        let visible = match visible_with_upper(mount, rel, Some(upper)) {
            Ok((shown, _)) => shown,
            Err(_) if !keeps_contents => None,
            Err(code) => {
                record_decision(Kind::Refuse, DecisionLayer::None, code, || {
                    mount.decision_path(rel)
                });
                return Err(Failure::Denied(code));
            }
        };
        if create_new_taken(create_new, false, visible.is_some()) {
            return Err(Failure::Denied(ERROR_FILE_EXISTS));
        }
        let path = match prepare_write_target(mount, rel, visible.is_some()) {
            Ok(target) => spelled_like(target, visible.as_ref()),
            Err(TargetFailure::Parent(failure)) => {
                record_decision(Kind::Missing, DecisionLayer::None, failure.win32(), || {
                    mount.decision_path(rel)
                });
                return Err(failure);
            }
            Err(TargetFailure::Folder(failure)) => {
                record_decision(Kind::Refuse, DecisionLayer::None, failure.win32(), || {
                    mount.decision_path(rel)
                });
                return Err(failure);
            }
        };
        if let Err(failure) = copy_up_shown(rel, &path, keeps_contents, visible) {
            record_decision(Kind::Refuse, DecisionLayer::None, failure.win32(), || {
                mount.decision_path(rel)
            });
            return Err(failure);
        }
        record_decision(Kind::Write, DecisionLayer::Upper, 0, || {
            mount.decision_path(rel)
        });
        trace(|| format!("write {rel} keeps={keeps_contents} -> {}", path.display()));
        return Ok(Target::Layer { path });
    }
    let shown = match probe_visible_layer(mount, rel, true, None) {
        Ok((shown, _)) => shown,
        Err(code) => {
            // A refused attribute query is reported as Windows reports it, not as a missing file.
            trace(|| format!("read {rel} refused {code}"));
            return Err(Failure::Denied(code));
        }
    };
    match shown {
        ShownLayer::Found {
            layer: Layer::Root, ..
        } if same_root => {
            trace(|| format!("read {rel} stays in the game folder"));
            Ok(Target::Same)
        }
        ShownLayer::Found { layer, .. } => {
            let Some(base) = mount.base(layer) else {
                return Err(Failure::Denied(ERROR_ACCESS_DENIED));
            };
            let path = join(base, rel);
            trace(|| format!("read {rel} -> {} from {layer:?}", path.display()));
            Ok(Target::Layer { path })
        }
        ShownLayer::Missing {
            reason,
            lower_checked,
        } => {
            // Every layer that a whiteout does not skip was already asked, and each attribute query told whether the
            // file or one of its folders is missing. Only a whiteout-hidden path still needs the parent query, because
            // it skipped the lower layers where that parent may live. A missing read is a routine probe, not a
            // mutation; the trace records it when `GOBBO_OVERLAY_LOG` is on.
            let missing = if lower_checked {
                reason
            } else {
                missing(mount, rel)?
            };
            trace(|| format!("read {rel} missing {missing:?}"));
            Err(missing.into())
        }
    }
}

/// Which step of the write rule refused, so each caller records the decision its own API owes.
enum TargetFailure {
    Parent(Failure),
    Folder(Failure),
}

/// Whether `make_parent` still has to ask the disk. `None` means nobody probed the parent, so it must; a `Found`
/// answer from the upper layer means the folder is already there and `exists` would only repeat that query.
fn make_parent_needed(upper_parent: Option<Probe>) -> bool {
    !upper_parent.is_some_and(Probe::is_found)
}

/// The write rule: check the logical parent, resolve the upper target, make its folder. `parent_checked` skips the
/// query when a visible file already proved the parent exists. The parent probe taken by `check_parent` is carried
/// into `make_parent`, so a cold write pays one query per parent layer instead of two for the upper one.
fn prepare_write_target(
    mount: &Mount,
    rel: &str,
    parent_checked: bool,
) -> Result<PathBuf, TargetFailure> {
    let upper_parent = if parent_checked {
        None
    } else {
        Some(check_parent(mount, rel).map_err(TargetFailure::Parent)?)
    };

    let target = mount.write_target(rel);
    if make_parent_needed(upper_parent) {
        make_parent(&target)
            .map_err(Failure::Io)
            .map_err(TargetFailure::Folder)?;
    }
    Ok(target)
}

/// The upper-layer path a write to `rel` goes to, with its folder made, so the caller can create the file there. The
/// logical parent must exist in a layer first: a write cannot create a missing folder on its own.
pub(crate) fn make_write_target(mount: &Mount, rel: &str) -> Result<PathBuf, Failure> {
    prepare_write_target(mount, rel, false).map_err(|failure| match failure {
        TargetFailure::Parent(failure) | TargetFailure::Folder(failure) => failure,
    })
}

/// Whether the folder that holds `rel` is a visible directory. The hook must check this before it creates any folder in
/// the upper layer: Windows fails a write into a missing folder, and the overlay must do the same. The returned probe
/// is the upper layer's answer about that folder, so the caller need not ask again when it makes the folder.
pub(crate) fn check_parent(mount: &Mount, rel: &str) -> Result<Probe, Failure> {
    let (visible, upper) =
        visible_with_upper(mount, parent_rel(rel), None).map_err(Failure::Denied)?;
    if visible.is_some_and(|(_, _, attributes)| attributes & FILE_ATTRIBUTE_DIRECTORY != 0) {
        Ok(upper)
    } else {
        Err(Missing::Folder.into())
    }
}

/// Whether the layers show `rel` as a folder.
pub(crate) fn is_visible_dir(mount: &Mount, rel: &str) -> bool {
    probe_visible(mount, rel)
        .ok()
        .flatten()
        .is_some_and(|(_, _, attributes)| attributes & FILE_ATTRIBUTE_DIRECTORY != 0)
}

/// Whether the folder that holds `rel` is shown by a layer. A file with no folder above it is `Missing::Folder`.
pub(crate) fn missing(mount: &Mount, rel: &str) -> Result<Missing, Failure> {
    match probe_visible(mount, parent_rel(rel)) {
        Ok(Some((_, _, attributes))) if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 => {
            Ok(Missing::File)
        }
        Ok(_) => Ok(Missing::Folder),
        Err(code) => Err(Failure::Denied(code)),
    }
}

/// The top-most layer where the game-relative path is visible, with the whiteouts applied.
pub(crate) fn visible(mount: &Mount, rel: &str) -> Option<(Layer, PathBuf)> {
    probe_visible(mount, rel)
        .ok()
        .flatten()
        .map(|(layer, path, _)| (layer, path))
}

/// The top-most layer where the path is visible, with its attributes. A query Windows refuses is an error, so a deletion
/// or a rename fails with the code Windows gave instead of treating the path as missing.
pub(crate) fn shown(mount: &Mount, rel: &str) -> Result<Option<(Layer, PathBuf, u32)>, Failure> {
    probe_visible(mount, rel).map_err(Failure::Denied)
}

/// The top-most visible layer of a path, with its full path and attributes.
type Visible = Option<(Layer, PathBuf, u32)>;

/// The top-most layer where the path is visible, with its attributes. An access refusal is returned, so a write can fail
/// with the code Windows gave instead of pretending the path is missing.
fn probe_visible(mount: &Mount, rel: &str) -> Result<Visible, u32> {
    visible_with_upper(mount, rel, None).map(|(shown, _)| shown)
}

/// `probe_visible` with the upper layer's answer the caller already has, returned alongside the visible layer. The
/// caller that probed upper before deciding to write must not pay for a second `GetFileAttributesW` on it.
fn visible_with_upper(
    mount: &Mount,
    rel: &str,
    known_upper: Option<Probe>,
) -> Result<(Visible, Probe), u32> {
    let (shown, upper) = probe_visible_layer(mount, rel, false, known_upper)?;
    let visible = match shown {
        ShownLayer::Found { layer, attributes } => {
            let Some(base) = mount.base(layer) else {
                return Err(ERROR_ACCESS_DENIED);
            };
            Some((layer, join(base, rel), attributes))
        }
        ShownLayer::Missing { .. } => None,
    };
    Ok((visible, upper))
}

/// The answer of the layer loop: the top-most layer that shows the path, or the reason the path is missing. A missing
/// answer tells whether the file or one of its folders is absent, as `GetFileAttributesW` already reported per layer.
enum ShownLayer {
    Found {
        layer: Layer,
        attributes: u32,
    },
    Missing {
        reason: Missing,
        lower_checked: bool,
    },
}

/// The probe a layer can reuse from the caller. Only the upper layer can have been asked already: the write path probes
/// it before it decides to write, and asking again in the layer walk would double the upper query on every cold write.
fn probe_for_layer(layer: Layer, known_upper: Option<Probe>) -> Option<Probe> {
    (layer == Layer::Upper).then_some(known_upper).flatten()
}

/// The top-most layer that shows `rel`, plus the upper layer's probe, without building a `PathBuf` for it. A read that
/// stays in the game folder does not need the path at all, and a missing path must not pay for one per layer.
/// `with_reason` asks for the exact `Missing` variant when no layer shows the path; a caller that only needs "missing"
/// passes false and pays no extra query. `known_upper` is the write path's first probe of the same path.
fn probe_visible_layer(
    mount: &Mount,
    rel: &str,
    with_reason: bool,
    known_upper: Option<Probe>,
) -> Result<(ShownLayer, Probe), u32> {
    mount.with_whiteouts(|whiteouts| {
        let hidden = whiteouts.hides(rel);
        // The upper folder is always part of the walk, whiteout or not, so its answer is taken once here and shared
        // with the caller and the write path. A path too long for the stack buffer still takes the allocating probe.
        let upper_probe = known_upper.unwrap_or_else(|| probe_child(&mount.upper, rel));
        // A layer whose query says the parent exists (`Missing::File`) makes the logical parent visible, unless a
        // higher layer has a non-directory at that parent and shadows it. That higher entry is the one case where the
        // parent still has to be asked; the layers above `first_file` are checked once the loop is done.
        let mut reason = Missing::Folder;
        let mut first_file = None;
        for (layer, base) in mount.layer_paths() {
            if layer != Layer::Upper && hidden {
                continue;
            }
            let probe =
                probe_for_layer(layer, Some(upper_probe)).unwrap_or_else(|| probe_child(base, rel));
            match probe {
                Probe::Found { attributes } => {
                    return Ok((ShownLayer::Found { layer, attributes }, upper_probe));
                }
                Probe::Denied(code) => return Err(code),
                Probe::Missing(missing) => {
                    if missing == Missing::File {
                        reason = Missing::File;
                        first_file.get_or_insert(layer);
                    }
                }
            }
        }
        // A whiteout hid the lower layers; their parent may still decide whether the file or the folder is missing.
        if hidden {
            return Ok((
                ShownLayer::Missing {
                    reason: Missing::Folder,
                    lower_checked: false,
                },
                upper_probe,
            ));
        }
        if with_reason && let Some(layer) = first_file {
            for (higher, higher_base) in mount.layer_paths() {
                if higher == layer {
                    break;
                }
                match probe_child(higher_base, parent_rel(rel)) {
                    // A directory here would have made the child query say `Missing::File`; any entry is a shadowing
                    // non-directory, so the merged path is missing a folder.
                    Probe::Found { .. } => {
                        return Ok((
                            ShownLayer::Missing {
                                reason: Missing::Folder,
                                lower_checked: true,
                            },
                            upper_probe,
                        ));
                    }
                    Probe::Denied(code) => return Err(code),
                    Probe::Missing(_) => {}
                }
            }
        }
        Ok((
            ShownLayer::Missing {
                reason,
                lower_checked: true,
            },
            upper_probe,
        ))
    })
}

/// Whether the mod layer or the game folder has the path, and no whiteout hides it. A query Windows refuses is an error,
/// so a deletion or a rename fails with the code Windows gave instead of leaving the lower entry in place.
pub(crate) fn in_lower(mount: &Mount, rel: &str) -> Result<bool, Failure> {
    mount.with_whiteouts(|whiteouts| {
        if whiteouts.hides(rel) {
            return Ok(false);
        }
        for (layer, base) in mount.layer_paths() {
            if layer == Layer::Upper {
                continue;
            }
            match probe_child(base, rel) {
                Probe::Found { .. } => return Ok(true),
                Probe::Denied(code) => return Err(Failure::Denied(code)),
                Probe::Missing(_) => {}
            }
        }
        Ok(false)
    })
}

/// The new upper copy of a file takes the spelling its lower file has on disk. NTFS keeps the spelling of a file that a
/// write reaches under another case, so a program that writes `A.TXT` to a file `a.txt` leaves `a.txt` in the listing.
/// `visible` is the lower file the write reaches, when a layer shows one.
fn spelled_like(target: PathBuf, visible: Option<&(Layer, PathBuf, u32)>) -> PathBuf {
    let Some((layer, source, attributes)) = visible else {
        return target;
    };
    if *layer == Layer::Upper || attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        return target;
    }
    match on_disk_name(source) {
        Some(name) if target.file_name() != Some(name.as_os_str()) => target.with_file_name(name),
        _ => target,
    }
}

/// The name a file has on disk, read from its directory entry, when the file exists.
fn on_disk_name(path: &Path) -> Option<std::ffi::OsString> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Storage::FileSystem::{FindClose, FindFirstFileW, WIN32_FIND_DATAW};

    let name = wide(path.as_os_str());
    // SAFETY: `name` is NUL-terminated and `data` is writable; the search handle is closed once.
    unsafe {
        let mut data: WIN32_FIND_DATAW = std::mem::zeroed();
        let handle = FindFirstFileW(name.as_ptr(), &mut data);
        if handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return None;
        }
        FindClose(handle);
        let length = data
            .cFileName
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(data.cFileName.len());
        Some(std::ffi::OsString::from_wide(&data.cFileName[..length]))
    }
}

/// The copy-up in progress for each target, so two threads that write the same lower file do not publish a half copy
/// or overwrite one another. Only copy-up uses it; the pass-through path never touches it.
static COPY_LOCKS: LazyLock<Mutex<FxHashMap<PathBuf, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));
/// The unique part of a temporary copy-up file.
static COPY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Makes the file exist in the upper layer before a write, so the write never reaches the game folder. The copy is
/// made only when the program keeps the old contents, and never from a file that a whiteout hides. The write path has
/// already checked that the logical parent is a folder; this only creates its counterpart in the upper layer.
pub(crate) fn copy_up(
    mount: &Mount,
    rel: &str,
    target: &Path,
    keeps_contents: bool,
) -> Result<(), Failure> {
    let visible = if keeps_contents {
        probe_visible(mount, rel).map_err(Failure::Denied)?
    } else {
        None
    };
    // This route serves a move or replace source: unlike the write path, nobody has made the target's folder yet.
    make_parent(target)?;
    copy_up_shown(rel, target, keeps_contents, visible)
}

/// `copy_up` with the visible file already probed by the write path. The probe is the expensive part on a cold layer;
/// the caller that found a lower file also knows its parent exists, so it does not ask again. The target's folder is
/// the caller's to make: `prepare_write_target` on the write path, `copy_up` for a move or replace source.
/// Remaining gap: the `create_new` fallback below restores attributes and timestamps, but not alternate data streams
/// or a full security descriptor. `fs::copy` (`CopyFileExW`) carries those on the hard-link path; a volume that cannot
/// hard-link still loses them. Keeping that gap is safer than opening the destination for more access than the write
/// the game asked for.
fn copy_up_shown(
    rel: &str,
    target: &Path,
    keeps_contents: bool,
    visible: Option<(Layer, PathBuf, u32)>,
) -> Result<(), Failure> {
    if !keeps_contents {
        return Ok(());
    }
    let Some((layer, source, attributes)) = visible else {
        return Ok(());
    };
    if layer == Layer::Upper {
        return Ok(());
    }
    if !copy_needed(
        keeps_contents,
        Some((layer, attributes & FILE_ATTRIBUTE_DIRECTORY == 0)),
    ) {
        return Ok(());
    }

    let lock = copy_lock(target);
    let copied = {
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        // Another thread may have finished the copy while this one waited for the lock.
        if exists(target) {
            Ok(())
        } else {
            let temporary = temporary_path(target);
            // `fs::copy` on Windows is `CopyFileExW`: it copies the data, attributes and timestamps, and falls back to
            // a read/write loop on a volume without it. A raw call here would add nothing.
            let copied = fs::copy(&source, &temporary)
                .map(|_| ())
                .and_then(|()| publish_copy(&temporary, target));
            let _ = fs::remove_file(&temporary);
            copied
        }
    };
    // Remove the per-target entry when no other writer holds it. The map lock makes the count and the removal atomic
    // with `copy_lock`, so a new writer either sees this entry or creates a fresh one after the file exists.
    if let Some(mut locks) = COPY_LOCKS.lock().ok()
        && Arc::strong_count(&lock) == 2
    {
        locks.remove(target);
    }
    copied?;
    record_decision(Kind::CopyUp, decision_layer(layer), 0, || rel.to_string());
    trace(|| format!("copy-up {rel} from {}", source.display()));
    Ok(())
}

/// The lock that serializes copy-ups of one target path.
fn copy_lock(target: &Path) -> Arc<Mutex<()>> {
    COPY_LOCKS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(target.to_path_buf())
        .or_default()
        .clone()
}

/// A unique temporary name in the target's folder, so the copy is complete before it gets the target's name.
fn temporary_path(target: &Path) -> PathBuf {
    let sequence = COPY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    target.with_file_name(copy_temp_name(std::process::id(), sequence))
}

/// Copies the attributes and timestamps from a completed temporary copy onto the destination of a fallback copy.
/// `fs::copy` already put the lower file's metadata on the temporary file; a byte copy into a new file does not. Both
/// writes are best effort: a volume that refuses one must not turn a successful copy into a failed game write.
fn copy_metadata(source: &fs::File, destination: &fs::File) {
    let Ok(metadata) = source.metadata() else {
        return;
    };
    // Set the times before the permissions: a read-only attribute can stop a later timestamp write.
    if let (Ok(accessed), Ok(modified)) = (metadata.accessed(), metadata.modified())
        && let Err(error) = destination.set_times(
            FileTimes::new()
                .set_accessed(accessed)
                .set_modified(modified),
        )
    {
        trace(|| format!("could not copy the timestamps of a copy-up: {error}"));
    }
    if let Err(error) = destination.set_permissions(metadata.permissions()) {
        trace(|| format!("could not copy the attributes of a copy-up: {error}"));
    }
}

/// Gives the completed temporary copy the target name without replacing a file that another thread or process created
/// first. A hard link is atomic and keeps the metadata of the copy; a volume without hard links falls back to a
/// `create_new` copy, which still never replaces an existing target.
fn publish_copy(temporary: &Path, target: &Path) -> io::Result<()> {
    match fs::hard_link(temporary, target) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(_) => {
            let mut source = fs::File::open(temporary)?;
            let mut destination = match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(()),
                Err(error) => return Err(error),
            };
            if let Err(error) = io::copy(&mut source, &mut destination) {
                drop(destination);
                let _ = fs::remove_file(target);
                return Err(error);
            }
            copy_metadata(&source, &destination);
            Ok(())
        }
    }
}

/// Whether a write copies the file up first. The program must keep the old contents, and the copy it would take from is a
/// file of a lower layer: the upper layer already holds the file, and a folder cannot be copied. `shown` is the layer of the
/// top-most visible copy and whether it is a file (`None` when no layer shows the file).
pub(crate) fn copy_needed(keeps_contents: bool, shown: Option<(Layer, bool)>) -> bool {
    keeps_contents && matches!(shown, Some((layer, true)) if layer != Layer::Upper)
}

/// Whether a rename or a move to a name must fail because the name is taken. It is not taken when the call replaces the
/// name, or when the destination is the source itself under another spelling (a case change). `shown` tells whether a
/// layer shows the destination, and it is asked only when the other two rules leave it open.
pub(crate) fn name_taken(same_entry: bool, replace: bool, shown: impl FnOnce() -> bool) -> bool {
    !same_entry && !replace && shown()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gobbo_overlay_core::{Layers, Whiteouts};

    fn mount(base: &Path) -> Mount {
        let layers = Layers {
            root: base.join("game"),
            staging: Some(base.join("mods")),
            upper: base.join("upper"),
        };
        Mount::new(
            "game".to_string(),
            layers.root.clone(),
            layers,
            Whiteouts::default(),
        )
    }

    #[test]
    fn concurrent_copy_ups_publish_one_complete_file() {
        let base = std::env::temp_dir().join(format!("gobbo-copy-up-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let mount = mount(&base);
        fs::create_dir_all(&mount.root).unwrap();
        fs::write(mount.root.join("a.bin"), "lower").unwrap();
        let target = mount.upper.join("a.bin");

        std::thread::scope(|scope| {
            for _ in 0..8 {
                let mount = &mount;
                let target = &target;
                scope.spawn(move || copy_up(mount, "a.bin", target, true).unwrap());
            }
        });

        assert_eq!(fs::read_to_string(&target).unwrap(), "lower");
        let leftovers: Vec<String> = fs::read_dir(&mount.upper)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".gobbo-copy-"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_refused_query_reports_the_code_windows_gave() {
        let failure = Failure::Denied(ERROR_ACCESS_DENIED);
        assert_eq!(failure.win32(), ERROR_ACCESS_DENIED);
        assert_eq!(failure.status(), STATUS_ACCESS_DENIED);
    }

    #[test]
    fn a_root_read_can_keep_the_caller_name_and_a_layer_read_cannot() {
        let base = std::env::temp_dir().join(format!("gobbo-target-same-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let mount = mount(&base);
        fs::create_dir_all(&mount.root).unwrap();
        let staging = mount.staging.clone().unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(mount.root.join("root.txt"), "root").unwrap();
        fs::write(staging.join("mod.txt"), "mod").unwrap();

        assert!(matches!(
            resolve(&mount, "root.txt", false, true, true, 0),
            Ok(Target::Same)
        ));
        assert!(matches!(
            resolve(&mount, "mod.txt", false, true, true, 0),
            Ok(Target::Layer { .. })
        ));
        // The NT path passes `same_root = false`, so even a root read gets an absolute name.
        assert!(matches!(
            resolve(&mount, "root.txt", false, true, false, 0),
            Ok(Target::Layer { .. })
        ));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_missing_file_and_a_missing_folder_keep_their_codes() {
        let base =
            std::env::temp_dir().join(format!("gobbo-target-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let mount = mount(&base);
        fs::create_dir_all(&mount.root).unwrap();
        fs::create_dir_all(&mount.upper).unwrap();
        fs::create_dir_all(mount.staging.as_ref().unwrap()).unwrap();
        fs::create_dir_all(mount.root.join("existing")).unwrap();

        // The folder exists in the root layer, so the file is missing rather than its path.
        let file = resolve(&mount, r"existing\missing.txt", false, true, false, 0);
        assert!(matches!(file, Err(Failure::Missing(Missing::File))));
        // The folder is missing in every layer, so the path is the missing part.
        let folder = resolve(&mount, r"absent\missing.txt", false, true, false, 0);
        assert!(matches!(folder, Err(Failure::Missing(Missing::Folder))));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_higher_file_at_the_parent_keeps_the_missing_path_a_folder() {
        let base = std::env::temp_dir().join(format!("gobbo-target-shadow-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let mount = mount(&base);
        fs::create_dir_all(&mount.root).unwrap();
        fs::create_dir_all(&mount.upper).unwrap();
        fs::create_dir_all(mount.staging.as_ref().unwrap()).unwrap();
        fs::create_dir_all(mount.root.join("shadow")).unwrap();
        // The upper layer has a file where the game folder has a directory. The upper file shadows that directory,
        // so a child below it is missing its folder, not just the file.
        fs::write(mount.upper.join("shadow"), b"file").unwrap();

        let missing = resolve(&mount, r"shadow\file.txt", false, true, false, 0);
        assert!(matches!(missing, Err(Failure::Missing(Missing::Folder))));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_maximum_or_security_right_asks_to_write() {
        assert!(writes_requested(MAXIMUM_ALLOWED));
        assert!(writes_requested(WRITE_DAC));
        assert!(writes_requested(ACCESS_SYSTEM_SECURITY));
        assert!(writes_requested(FILE_WRITE_EA));
        assert!(!writes_requested(0x0001));
        assert!(!writes_requested(0x8000_0000));
    }

    #[test]
    fn the_parent_of_a_relative_path_is_the_folder_above_it() {
        assert_eq!(parent_rel(""), "");
        assert_eq!(parent_rel("a"), "");
        assert_eq!(parent_rel(r"a\b"), "a");
        assert_eq!(parent_rel(r"a\b\c"), r"a\b");
    }

    #[test]
    fn a_write_copies_up_only_a_lower_file_that_it_keeps() {
        assert!(copy_needed(true, Some((Layer::Root, true))));
        assert!(copy_needed(true, Some((Layer::Staging, true))));
        // The upper layer already has the file, a folder cannot be copied, and a truncating write keeps nothing.
        assert!(!copy_needed(true, Some((Layer::Upper, true))));
        assert!(!copy_needed(true, Some((Layer::Root, false))));
        assert!(!copy_needed(false, Some((Layer::Root, true))));
        assert!(!copy_needed(true, None));
    }

    #[test]
    fn a_name_is_taken_only_when_shown_and_not_replaced_or_the_same_entry() {
        assert!(name_taken(false, false, || true));
        assert!(!name_taken(false, false, || false));
        // Replacing the name, or a case change of the same entry, is not a taken name.
        assert!(!name_taken(false, true, || true));
        assert!(!name_taken(true, false, || true));
    }

    #[test]
    fn a_name_is_not_asked_about_when_another_rule_settles_it() {
        let mut asked = false;
        assert!(!name_taken(true, false, || {
            asked = true;
            true
        }));
        assert!(!name_taken(false, true, || {
            asked = true;
            true
        }));
        assert!(!asked);
    }

    #[test]
    fn an_upper_probe_is_reused_only_for_the_upper_layer() {
        let probe = Probe::Missing(Missing::File);
        assert_eq!(probe_for_layer(Layer::Upper, Some(probe)), Some(probe));
        assert_eq!(probe_for_layer(Layer::Staging, Some(probe)), None);
        assert_eq!(probe_for_layer(Layer::Root, Some(probe)), None);
        assert_eq!(probe_for_layer(Layer::Upper, None), None);
    }

    #[test]
    fn a_known_upper_answer_is_used_without_a_second_query() {
        let base = std::env::temp_dir().join(format!("gobbo-target-upper-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let mount = mount(&base);
        fs::create_dir_all(&mount.upper).unwrap();
        fs::create_dir_all(&mount.root).unwrap();
        let known = probe_child(&mount.upper, "a.txt");
        assert_eq!(known, Probe::Missing(Missing::File));
        fs::write(mount.root.join("a.txt"), b"lower").unwrap();
        // An upper file appears after the caller probed. The layer walk must use the caller's answer, so the file it
        // reports is still the lower one, and the returned probe is exactly the one it was given.
        fs::write(mount.upper.join("a.txt"), b"upper").unwrap();
        let (visible, upper) = visible_with_upper(&mount, "a.txt", Some(known)).unwrap();
        assert_eq!(upper, known);
        assert!(matches!(visible, Some((Layer::Root, _, _))), "{visible:?}");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn make_parent_is_skipped_only_when_the_upper_parent_already_exists() {
        assert!(make_parent_needed(None));
        assert!(make_parent_needed(Some(Probe::Missing(Missing::File))));
        assert!(make_parent_needed(Some(Probe::Missing(Missing::Folder))));
        assert!(make_parent_needed(Some(Probe::Denied(ERROR_ACCESS_DENIED))));
        assert!(!make_parent_needed(Some(Probe::Found {
            attributes: FILE_ATTRIBUTE_DIRECTORY
        })));
        // A found file is still an existing path, as `exists` would report it.
        assert!(!make_parent_needed(Some(Probe::Found { attributes: 0 })));
    }

    #[test]
    fn a_create_new_name_is_taken_when_the_probes_show_it() {
        assert!(!create_new_taken(false, true, true));
        assert!(!create_new_taken(true, false, false));
        assert!(create_new_taken(true, true, false));
        assert!(create_new_taken(true, false, true));
    }

    #[test]
    fn a_create_new_over_a_visible_file_refuses_before_making_the_upper_folder() {
        let base = std::env::temp_dir().join(format!("gobbo-target-new-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let mount = mount(&base);
        fs::create_dir_all(&mount.root).unwrap();
        fs::write(mount.root.join("lower.txt"), b"lower").unwrap();

        let lower = resolve(&mount, "lower.txt", true, true, false, CREATE_NEW);
        assert_eq!(
            lower.err().map(|failure| failure.win32()),
            Some(ERROR_FILE_EXISTS)
        );
        assert!(!mount.upper.exists());

        fs::create_dir_all(&mount.upper).unwrap();
        fs::write(mount.upper.join("upper.txt"), b"upper").unwrap();
        let upper = resolve(&mount, "upper.txt", true, true, false, CREATE_NEW);
        assert_eq!(
            upper.err().map(|failure| failure.win32()),
            Some(ERROR_FILE_EXISTS)
        );
        assert_eq!(
            fs::read_to_string(mount.upper.join("upper.txt")).unwrap(),
            "upper"
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_fallback_copy_keeps_the_modified_time() {
        let base = std::env::temp_dir().join(format!("gobbo-target-meta-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let from = base.join("temporary.bin");
        let to = base.join("target.bin");
        fs::write(&from, b"data").unwrap();
        let older = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let writable = fs::OpenOptions::new().write(true).open(&from).unwrap();
        writable
            .set_times(FileTimes::new().set_modified(older))
            .unwrap();
        drop(writable);

        let source = fs::File::open(&from).unwrap();
        let destination = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&to)
            .unwrap();
        copy_metadata(&source, &destination);
        drop(source);
        drop(destination);

        let expected = fs::metadata(&from).unwrap().modified().unwrap();
        let copied = fs::metadata(&to).unwrap().modified().unwrap();
        assert_eq!(copied, expected);
        fs::remove_dir_all(&base).unwrap();
    }
}
