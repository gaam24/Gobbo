# Hook table and ownership

The table is in `hooking/table.rs` (today `detours/mod.rs`). Each entry has a field name (the `GOBBO_OVERLAY_DISABLE`
name), a signature, the module/export, the hook and a status. The same hooks serve the game mount and every user-data
mount; adding a user-data set does not add a hook.

| Area          | Hook                                                                                                                                          | Why                                                                                                                                                                                                                                                                                                            | Essential                                       |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| Open          | `CreateFileW`, `NtCreateFile`, `NtOpenFile`                                                                                                   | Every file open; NT covers `CreateFile2` and the loader                                                                                                                                                                                                                                                        | yes                                             |
| Attributes    | `NtQueryAttributesFile`, `NtQueryFullAttributesFile` (the Win32 `GetFileAttributes*` calls go through them)                                   | Existence/type checks                                                                                                                                                                                                                                                                                          | yes                                             |
| Delete/rename | `DeleteFileW`, `RemoveDirectoryW`, `MoveFileExW`, `MoveFileWithProgressW`, `NtSetInformationFile`                                             | Deletions and renames through paths and handles                                                                                                                                                                                                                                                                | yes                                             |
| Listing       | `FindFirstFileW`, `FindFirstFileExW`, `FindNextFileW`, `FindClose`, `NtQueryDirectoryFile`, `NtQueryDirectoryFileEx`                          | Merged directory view from one `FindFirstFileExW` per layer with the caller's mask; `.` and `..` come once, first, from `dot_entries` (a plain NTFS folder lists them too); the NT hooks format the same merged entries in the six supported `FILE_*_INFORMATION` classes, and an unknown class passes through | yes                                             |
| Replace/link  | `ReplaceFileW`, `CreateHardLinkW`, `CreateSymbolicLinkW`                                                                                      | Refuse links into the game folder; replace upper copies                                                                                                                                                                                                                                                        | no                                              |
| Process       | `CreateProcessInternalW`; fallback `CreateProcessW`, `CreateProcessAsUserW` when the internal export is missing or disabled                   | Hook children                                                                                                                                                                                                                                                                                                  | yes (one of them)                               |
| Handles       | `GetFinalPathNameByHandleW`, `GetMappedFileNameW`, `GetModuleFileNameW`, `GetModuleFileNameA`, `GetModuleFileNameExW`, `GetModuleFileNameExA` | Map the actual final path, the NT name of a mapped file and the path of a module loaded from a layer back to the game path; `A` covers programs that ask for the ANSI form, `Ex` the process-module form used with `EnumProcessModules`                                                                        | no (mapping only; launch does not depend on it) |
| Info          | `GetFileInformationByHandle`, `NtQueryInformationFile`                                                                                        | `GetFileInformationByHandle` is trace only; `NtQueryInformationFile` rewrites `FileNameInformation` and `FileNormalizedNameInformation` to the game path for a handle in a layer, and traces the class and status when tracing is on                                                                           | no                                              |

The Win32 wrappers that the NT layer already answers were measured against a scripted session
(`the_answers_of_a_scripted_session_are_the_expected_ones`, `program_calls.rs` and `game_files.rs`): listings, attributes,
deletions, removals, renames and errors give the same answers with them off. `GetFileAttributes(Ex)W` was removed for that
reason, and it costs nothing on a pass-through call. The Win32 hooks that stay, and why:

- `DeleteFileW`: kept for speed, not for parity. Without it a pass-through deletion takes two NT calls through the hook
  instead of one, and the whiteout row of the benchmark went from about 90 to about 129 µs (p50, one run). The answers
  are the same.
- `CreateFileW`: refuses a device-namespace write (`\\.\C:`). The system turns that name into `\??\C:\...` before the NT
  layer sees it, so the refusal can only be made here.
- `MoveFileExW`, `MoveFileWithProgressW`: a rename by path. A rename of a file through an open handle is not redirected
  (see the limits above), and `MOVEFILE_DELAY_UNTIL_REBOOT` would schedule a change to the game folder at the next boot.
- `RemoveDirectoryW`: removing a folder of a lower layer leaves a whiteout. The NT path leaves the folder in place.
- `FindFirstFile*`, `FindNextFileW`, `FindClose`: the NT directory query answers `attrib` and `move` differently
  (`attrib` reports a symbolic link that does not exist; `move` fails) when these hooks are off. The cause is not yet
  known; the removal waits for it.

Rules for adding a hook:

- Prefer the lowest layer that covers all callers (`NtCreateFile` over `CreateFileW`; `NtQueryDirectoryFile` over
  `FindFirstFileW`). Do not hook both unless you can explain why the higher one is still needed.
- A new hook must have: a count, a test that proves the redirection, and a status in the session report; a hook that
  mutates or produces a merged view also needs a decision record.
- Each hook is created independently. A missing export is a failed status for that hook only; it must not disable the
  rest. The launcher resumes the game only when the essential set (`ESSENTIAL` in `lib.rs`) is active.
- `state::hooks()` returns `Option`; a hook that cannot find its original returns the API's failure instead of panicking.
- Do not hook a hot API just to trace it. Tracing a hot API is a separate, opt-in mode.
- Do not keep a process-wide handle table. `GetFinalPathNameByHandleW` maps the actual final path of the handle
  (`upper`/`staging` prefix to `root`), so no `CloseHandle`/`NtClose` hook and no per-handle state are needed. The old
  table was removed; do not add it back for a new case without a test that proves the mapping cannot work.
- The one exception is the NT directory cursor: `NtQueryDirectoryFile(Ex)` is stateful, and a merged listing must
  remember the next entry between calls. `nt/directory.rs` keeps a bounded table (128 entries) keyed by the handle and
  its game-relative folder, evicts the least recently used listing, and rebuilds on `RestartScan` or when a reused
  handle names another folder. Each listing holds the upper/staging entries, a `RootStream` and the handle's final path
  in UTF-16, and pulls 64 to 128 entries into a batch per response, so memory follows the mod layers, not the size of
  the game folder. A matching final path continues the snapshot without re-probing the layers; a reused handle for the
  same folder keeps the old cursor. A caller that reopens a folder must start with `RestartScan`, the documented NT
  idiom. Do not grow this into a general handle table.
- The PEB module list (`LDR_DATA_TABLE_ENTRY.FullDllName`) still holds the real staging path of a mod DLL. `EnumProcessModules`
  returns only handles, so the path APIs paired with it are covered. A direct walk of the PEB is an accepted gap: the
  loader and `FreeLibrary` use those names, and rewriting them in place can make an unload target a path that is not the
  loaded file. Do not add a PEB rewrite without an x64 and x86 test, including unload and a child process.
