# Path model

The path layer is the most important part of the crate. Do not add another ad-hoc conversion.

## 5.1. Types

- `Mount` (`state.rs`) - one guest path (`guest`), the `Layers` that serve it (`root`, optional `staging`, `upper`) and
  its own whiteout snapshot. `Mounts` holds the game mount first and the user-data redirects after it.
- `MountConfig` (`overlay-core`) - the JSON file named by `GOBBO_OVERLAY_MOUNTS`, version 1, the game mount first and
  at most seven redirects. The hook refuses an unknown version, a missing game mount, a relative path or a duplicate id.
  Without the file it falls back to `GOBBO_OVERLAY_ROOT/STAGING/UPPER` for one game mount. `session_config::load`
  converts every path to its long form (`paths::long_path`): a program may spell a folder with 8.3 names, but the APIs
  that name handles and modules report the long form, and comparing a short prefix against a long answer would make
  every layer handle look external. `MountPaths` still caches the 8.3 alias so both incoming spellings classify.
- `MountPaths` (`paths/session.rs`) - the cached guest path of one mount plus `guest_utf16` (without a trailing
  separator, compared case-insensitively by the fast path) and `short_guest_utf16` (the 8.3 form when Windows gives one
  that differs). Built during install and never rebuilt.
- `Place` (`paths/names.rs`) - the `Mount` a name belongs to and the path relative to its guest. `place`/`place_units`
  replace the old single-root `relative_in`.
- `Probe` (`paths/probe.rs`) - the result of one `GetFileAttributesW`: `Found { attributes }`, `Missing(Missing)` or
  `Denied(code)`.
- `Missing` - `File` (the parent folder exists) or `Folder` (a folder above the name is missing); it carries the Win32
  and NT status Windows would report.
- `Failure` - `Missing(Missing)`, `Denied(code)` from an attribute query, or `Io(error)` from a copy-up. `win32()` and
  `status()` map it for the Win32/NT callers.
- `Target` - the layer path to pass to the original call. The game-relative path is classified again only when the
  `GOBBO_OVERLAY_CALLS` report is on, so a redirected open does not allocate it in a normal session.

## 5.2. Classification

`place`/`place_units` classify a name against every mount; `absolute` resolves text that needs a full path:

1. Null or empty -> `None`; the caller passes the call through or fails it.
2. Absolute drive/UNC path -> compare the UTF-16 prefix of each `guest` case-insensitively, without allocating. A prefix
   match followed by `\` or the end is inside; the game mount is tried first, then the redirects. A name outside every
   mount returns `None` with no allocation. The 8.3 guest alias is tried after the long form. A short name with `..`,
   `.` or a trailing separator still takes the slow path and is not remapped; that remains a known edge case. `\\?\`
   drive paths are compared without the prefix; `\\?\UNC\`, dotted paths and names with `/` fall back to the full path
   code.
3. Relative, rooted or drive-relative path -> `GetFullPathNameW` once (stack buffer, grown if needed), then the same
   comparison. Do not use `current_dir().join`: it gets `/foo`, `C:foo` and deleted current directories wrong.
4. `is_plain` keeps the fast path to names `normal` would return unchanged: no empty component, no `.`/`..`, no trailing
   separator. Anything else goes to `relative_to`, which normalizes it.
5. NT object names are classified from their UTF-16 units after `\??\` is stripped (`nt/names.rs`). A name relative to
   a directory handle still joins the program path of that handle. `nt/names.rs` returns a `Place`, so the NT layer
   resolves against the mount the object belongs to.
6. Device namespaces that can alias a game file are refused for a write, delete, rename, replacement or link:
   `\\.\C:`, `\\.\UNC`, `\\.\Volume{...}`, `\\?\GLOBALROOT`, `\\?\Volume{...}`, `\??\GLOBALROOT`,
   `\??\Volume{...}` and `\Device\`. `unsafe_units` implements this; a named pipe or mailslot is not flagged, and a
   read may pass through. The check runs before `check_parent`/`copy_up`, so an uncertain device name never reaches the
   game folder.
7. A name that cannot be resolved returns `None`. Writes/deletes/renames then fail closed through `check_parent` and
   `copy_up`; reads may pass through or return the Windows error, never a guessed path.
8. Long paths: `probe::verbatim_wide` adds `\\?\`/`\\?\UNC\` for an absolute path over `MAX_PATH`; `absolute` keeps an
   absolute name as it came. A path the program already gave is never rewritten into another spelling.

- **Name folding.** Every comparison of a name (a prefix of a mount, a whiteout key, a listing duplicate, a rename onto
  itself) uses one rule, `gobbo_overlay_core::fold_char` / `fold_unit` / `fold_key` / `fold_key_into` / `same_folded`:
  the hot whiteout and listing checks fold into a stack buffer (`fold_key_into`, `fold_child_into`,
  `fold_child_units_into`) and allocate only when the folded path does not fit. ASCII folds as
  Windows folds it, and any other letter takes its lower-case form when that form is one character (`ś` for `Ś`). The
  app's path keys (`domain/pathkey.rs`) lower-case the same way, except for the few letters whose lower case is several
  characters, so a name the program spells in another case finds its file
  on both sides. Syntax stays ASCII: a drive letter, `\?\`, `UNC\` and the keywords of device names fold ASCII only
  (`fold_ascii` in `paths/names.rs`), so a letter outside ASCII is never a drive. Known residual: NTFS folds by its own
  upper-case table, which can differ from Unicode lower case for a few letters; a name that differs only there is
  treated as another name.

## 5.3. Resolution

- `shown(mount, rel)` -> `Result<Option<(Layer, PathBuf, attrs)>, Failure>`: the top-most visible layer of that mount,
  with a Windows refusal preserved. One probe per layer, short-circuit. `visible` is the lossy `Option` used only where
  a refusal may be treated as "not visible".
- `in_lower(mount, rel)` -> `Result<bool, Failure>`: staging/root (or the user-data base) only, whiteout-aware, a
  refused query is an error.
- `missing(mount, rel)` -> `Result<Missing, Failure>`: `Missing::File` when the parent exists as a visible directory,
  `Missing::Folder` otherwise. `check_parent` is the write-side version.
- `resolve` (in `paths/target.rs`): a write checks the logical parent **before** creating any folder in `upper`, copies
  up when the program keeps the old contents, and returns the upper target; a read uses `shown` and reports the Windows
  refusal instead of pretending the file is missing.
- `copy_up` is needed only when `keeps_contents` is true and the top-most visible copy is a lower-layer file. It copies
  to a temporary file in the same upper directory with `fs::copy` (`CopyFileExW` on Windows), publishes it with a hard
  link or `create_new`, and removes the temporary file; a per-target lock serializes two writers, and its map entry is
  removed when the last writer leaves. A failure is returned to the caller, never swallowed.
- The temporary file has the name `copy_temp_name` (`.gobbo-copy-<pid>-<n>.tmp`, `overlay-core`). Listings hide that
  shape in every layer, and the launcher (`deploy/overlay/windows/mod.rs`, `sweep_stale_copies`) removes the files an
  interrupted run left, before a launch and only when no process runs from the game folder.
- The new upper copy takes the on-disk spelling of the lower file it copies (`spelled_like`, one `FindFirstFileW` on the
  lower file), and a truncating write to a lower file takes it too: NTFS keeps the spelling of an existing file, so a
  program that writes `A.TXT` over `a.txt` leaves `a.txt` in its listing. Only the leaf is taken here.
- The listing shows the spelling of every component the same way: an entry that a lower layer has under the same name, and
  that no whiteout hides, is listed with the lower spelling (root, then staging, then upper; `search/listing.rs`
  `apply_spellings`). A small set (at most 8 upper/staging entries) asks each lower layer for the exact name through
  `paths/spelling.rs`; a larger one reads the root folder once and reuses that spelling for every taken name, because
  one exact search per mod file cost a disk search per entry. The mask is built in UTF-16, so no `String` is allocated
  per entry. The copy in the plain tree of the oracle does the same, because NTFS keeps the name of an existing entry
  when a file is written over it.
- A move or rename that replaces a file of a lower layer hides that lower file once it succeeds (`win32/move_file.rs`,
  `nt/set_info.rs`), as a delete does. The replacement keeps the spelling the program gave it, as NTFS does for a move.
- A move, replace or rename may not cross two mounts: the copy-up and the whiteout would belong to different profile
  layers. Such a call fails with `ERROR_ACCESS_DENIED` (or the NT `STATUS_ACCESS_DENIED`) and records a refusal.
- `copy_needed`, `name_taken` and `writes_requested` are pure and unit-tested without a session.

## 5.4. Whiteouts

- Every mount has its own `Whiteouts`, a set of lowercase paths relative to that mount's guest. `hides(rel)` checks the
  path and every parent prefix. A deletion in one profile never hides a file in another.
- Readers take an `Arc<Whiteouts>` snapshot under a short read lock and probe the disk after the lock is gone. Do not
  hold a lock while probing.
- Writers change the snapshot under a short write lock and append one complete line to `<upper>.whiteouts.journal`.
  Every 512 lines the journal is folded into `<upper>.whiteouts` with temp + rename and truncated. A final journal line
  without a newline is ignored and cut off on load; it is an interrupted write, not a path.
- A failed append disables the journal for that mount but keeps the in-memory snapshot. A failed compaction is logged
  and retried; the old lines replay harmlessly because the set ignores duplicates.
- The upper layer is never hidden. A file the game writes again is visible.
- A rebuilt layer removes both `<upper>.whiteouts` and `<upper>.whiteouts.journal`. A profile switch never rebuilds a
  redirect's upper; it is the profile's data.
