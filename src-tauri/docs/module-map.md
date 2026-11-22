# Module map

- `app/mod.rs`: `run()`, the Tauri builder, plugins and setup. `app/cli.rs`: `gobbo run <game> -- <program>`.
  `app/storage_prompt.rs`: the first-start storage question, shown before Tauri so the data root, window state and
  webview profile are fixed before the window exists. Its `storage_prompt/native/` holds the platform dialog: GTK on
  the main thread on Linux, `rfd` on Windows and macOS. `app/bindings.rs`: writes and checks `../src/bindings.ts`.
- `bench_api.rs`: fixtures for `benches/backend.rs` (including a realistic multi-version library with long
  descriptions and a deployed journal); compiled only with the `bench` feature, never in a release build.
  The criterion harness is `benches/backend.rs`; `benches/alloc.rs` counts allocations with `dhat` (and
  `tests/alloc_budget.rs` keeps the ceilings), `benches/instructions.rs` runs `iai-callgrind` on Linux, and the
  `bench-mimalloc` feature swaps the allocator of the benchmark binary for the F6 experiment.
- `commands/`: `mod.rs` keeps the shared `blocking`, `for_game` and `change` helpers and the `game_commands!` macro. The
  handler list itself comes from `src-tauri/commands.txt`, the one registry `build.rs` uses to generate both the
  `collect_commands![...]` invocation `builder()` includes and the command permissions. Every command module lives in
  the module it serves; this layer is only the registration point.
- Modules: each folder owns its commands, use cases and engine. `games/` (the definition model in `definition/`, the
  loader presets in `loaders.rs`, validation, the scan in `detect.rs`, one adapter per store in `stores/`
  (`Store::display_name` and `Store::from_name` are read by `status/build.rs` for the store shown in the window;
  Steam's libraries and Proton data in `stores/steam.rs`; where each store keeps its files comes from
  `platform::store_folders`),
  how an install starts in `runtime.rs`, paths, the catalog
  in `catalog.rs`, the official catalog downloaded from GitHub in `remote/` (the manifest fetch, the schema gate and
  the checksummed install), the catalog commands and the definition editor in `editor/`; the import dry run is
  `import/preview.rs`),
  `library/` (mutations: rescan, delete, metadata, the manual source id in `source_id.rs`, generic `fields.rs`,
  `relations.rs` for the requirement checks and warnings, `sync.rs` keeping each profile in step
  with the library, `waiting.rs` for pack mods that wait for an archive and the settle step that puts them in the list), `deploy/` (`plan.rs`, `apply.rs`,
  `progress.rs` with the one progress and throttling rule for an apply, `error.rs`, `journal.rs` with the undo log,
  the `deployment.json` completion marker and the `JournalRoots` check (only a valid marker of this or an older schema
  counts as applied; a missing, damaged or newer one leaves an interrupted apply for undo or the next apply to repair;
  a torn last line or an unknown `op` is skipped and logged instead of blocking undo; every entry path is confined to
  the game root, the profile's layer folders and the settings file before undo changes anything),
  preview, target, overwrite, `remove.rs` for removing a mod that may be deployed; `overlay/` for the layer and the
  hardlink helper that builds it), `acquire/` (the one path that brings a mod archive into the library for every feature: exact-file check, fetch,
  FOMOD choices, import, download cleanup), `import/` (`model.rs` with `ChoiceNeeded`, the archive and FOMOD answers
  needed before an import can finish, the install plan and its `apply.rs`, `install/roots.rs` with `base_roots` for the
  game's top-level folders, FOMOD parser, tool unpack, wizard, reinstall, `report.rs` for the
  archive-versus-installed-folder report, `skipped.rs` for the capped record of files no rule took, and `flow/` with the
  archive lifecycle: working folder, preparation and commit), `launch/` (start, Proton, tools, steps, changes,
  report, output, watch, and `commands.rs` for starting, stopping and running a game), `infra/disk.rs` (free space for
  the setup check), `profiles/` (`actions/`, with the list and lifecycle in `mod.rs` and the user-data move in
  `user_data.rs`, bisect
  steps, compare, MO2, order, the history preview, `transfer.rs` for copying one profile's data into another, and one
  `commands.rs`), `packs/` (the reader with its `import/` folder for bundled archives), `collections/` (the collection
  recipe of any provider with the capability: `manifest/` reads and validates `collection.json`, `inspect.rs` opens a
  revision and its archive, `status.rs` derives each member's state through the shared `LibraryView::find`, `session/`
  owns the profile and its waiting entries, `download/` runs the automatic and guided loops with one `import_member`
  step that brings a member in through `acquire::acquire` (which imports via `import::import_batch`;
  `import::import_archive` is only the single-archive wrapper for tests and benches), matching members through
  `LibraryView::find`; `commands/` holds the collection commands, and `bundle.rs`, `external.rs` and `patch.rs` handle
  bundled files, direct URLs and curator patches), `sources/` (provider commands,
  metadata refresh, provider-neutral update checks, and the shared download queue in `downloads/` (`queue/` with its
  state, link resolution, dedup, claim and install parts), `status/`
  (read model, with the status builder in `build/` (its `mods.rs` and `tools.rs` parts), the one derived game stage in `stage.rs`, the setup checklist in
  `preflight.rs`, the bisect view in `bisect.rs`, the step choices in `steps.rs`, the mod views in `mod_views/` and the
  view types in `views/`), `tray/` (the native system tray: `view.rs` builds the menu description from the status,
  tasks and running games, `menu.rs` turns it into a native menu, `actions.rs` calls the existing use cases, `labels.rs`
  holds the words the window sends, and `state.rs` remembers the last game and busy state), `settings/`
  (app settings, the OS handlers of the provider download links in `links.rs`,
  the games the user added, restored, removed and their storage in `managed.rs`), `setup/` (the loader package a game
  can install: the setup view, the verified download, its import as a library mod and turning it on), `diagnostics/`, `snapshots/` (the
  snapshot manifest, capture, restore and view, with the command entry points in `mod.rs`; a snapshot holds the
  profile's list and user-data choices, the legacy store, every isolated set and the overwrite layer), `overwrite/`
  and `mods/`.
  `launch/pipeline.rs` is the one recorded run shared by a game, a tool and `gobbo run`: report, steps, start, wait,
  exit and profile capture. Shared rules stay in `domain`, I/O in `infra`, OS differences in `platform` and external
  services in `providers`.
- `migrations/`: the file-layout migrations, run once at startup and before the games list is read. `game_ids.rs` renames
  the data folders of games whose ids changed, `games_list.rs` turns the old `games.json` into the folder list, and
  `snapshots.rs` moves automatic snapshots written under a game folder to `snapshots/<game>/`. A new migration is one
  more step in `migrations/mod.rs`, idempotent and safe to run on every start.
- `context/`: the shared application context. `data/mod.rs` holds `Data`, `Loaded`, `open`, the load functions and the
  caches; `data/edit.rs` holds `edit_library`, `edit_library_clean`, `edit_profiles` and `idle_data` (the guard that
  refuses a game with a prepared deployment), and `data/load.rs` the reads every view starts with. `settings.rs` the game settings shape, read and save,
  `managed.rs` the games the user added (the `games/` folders, with the file-layout migrations applied first), `target.rs` the
  path resolution, `running.rs` the
  games started here, `library/` the library persistence as the shared context reads it: `index/mod.rs` with the
  queries and the `library.json` load/save, `index/scan.rs` with the scan, `folders.rs`, and the index lock. Its rescan
  takes the archive naming as a parameter, so the context does not depend on the providers. The pure model and view
  live in `domain::library`. `profile/`
  the copy list, user-data sets, owner files and their shared switching lifecycle in `store.rs` (each switch is
  journaled and recovers an interrupted one), the kept history and the bisect search file. `profiles/actions/user_data.rs` also
  moves an isolated redirect set back into the real folder and reports its on-demand size. `Data`
  keeps the read-through caches of `library.json`, `profiles.json`, `config.json`, the library folder listing and the
  journal's owned paths, and the index/folder comparison while both inputs stay the same (`infra/memo.rs`). The game
  definition has one process-wide cache in `games/mod.rs`, shared through `Arc`. A status reads the shared `Arc` of the
  parsed profiles when it is already in step with the library,
  so it does not clone the whole list. Every
  save of `profiles.json` goes through `Data::save_profiles`, which puts the written value into the cache: two quick
  saves of the same length can share a modification time. A change of
  the index goes through `edit_library` (lock, read, change, save), so it never bypasses the lock or the cache.
  Modules import `crate::context` for shared state.
- `tests/`: the backend integration suite (`tests.rs` and `tests/`), compiled only under `#[cfg(test)]`. It drives the
  same public module functions as the window.
- `domain/`: `archive.rs` (the pure parser for Nexus archive names and the one archive-extension rule), `deploy.rs`
  (the `Deploy`, `Slot`, `Mirror` and `Shared` rules a `game.toml` is parsed into, and the questions `resolver` asks
  about them), `deployment.rs` (the `deployment.json` record and the schema version this build trusts),
  `library/` (the pure `Library`, `Family`, `ModVersion`, `Archive`, `ModInfo` and `LibraryView`, whose `view/find.rs` is the one rule that
  matches a wanted mod to the library through `find`, `find_with` and its `FindQuery`/`MatchPolicy` for the strict,
  folded and key comparisons; the disk lives in `context::library`), `source/`
  (provider-neutral `ProviderId`, `SourceRef`, `SourceMod`, `SourceMeta`, `SourceRequirement`, `SourceSync`;
  `SourceMeta::checked` in `page.rs` is the one merge-and-stamp rule for a page a check stored;
  `SourceRef.part_id` names a provider's persistent installable file group, such as a Nexus `mod_file_id`, so two parts
  of one page stay separate; the library, profiles and packs store this model, and each provider maps its own records
  to it), `resolver/` (the deployment plan: `planner.rs` with `PlanMode`, `model.rs`, `shared.rs`), `profiles/`
  (`profile.rs` for one profile and its list, `set/` for the set and its operations, `entry.rs` for one mod in a
  profile, `view.rs` for the family/version indexes, `diff.rs` for the one comparison of two lists, `history.rs` for
  the compact summary of a list change),
  `user_data.rs` (the universal `Kind`/`Mode`/`Isolation` and the resolved `Set`),
  `naming/` (library and game folder names, safe file names and the relative-path rule), `pathkey.rs`, `version.rs`,
  `mod_steps.rs` (the per-mod step file and the `Phase` the launch runs), `arguments.rs`, `fomod.rs` (the wizard
  choices, keyed by step and group position, kept with a version), `pack.rs` (a pack mod and its waiting entry; the
  pack reader and writer live in `packs`), `storage.rs` (the shared `StorageMode`).
- `infra/`: `store.rs` (archive names, reservations, atomic writes, stale temp cleanup), `json.rs` (the one read and
  write policy for state files: a temp file is synced before its rename and the parent folder is synced on Unix),
  `deployment.rs` (reading the completion marker: a missing, damaged or newer one is not applied and stays on disk),
  `storage.rs` (the portable/global/custom data roots and the choice file),
  `credentials.rs` (the OS keyring, one account per provider), `io.rs`
  (chunked copies, progress and digests), `text.rs` (the tail reader), `fs/` (the shared walk, file listings,
  removal and tree copy; a replace is staged and keeps one previous generation for a profile store), `clock.rs` (the
  one wall clock and the RFC 3339 parser),
  `file_cache.rs` (the one-entry cache keyed by a path's length and modification time), `profiles_file.rs`
  (`profiles.json`), `archive.rs` (zip, 7z, rar), `http/` (the shared client and download), `log.rs` (the app's `gobbo.log`, one rotated backup and
  redacted request secrets).
- `platform/`: `mod.rs` (the names both platforms export), `linux.rs` and its `linux/proton.rs` (Steam's Proton
  folders, the keys and display names in `compatibilitytool.vdf`, and the library fallback), `windows.rs`, and
  `mount_table.rs` (the Linux mount-table parser the overlay session check reads). The Linux process list owns the
  overlay session check, so the platform layer never imports `deploy`.
- `providers/`: the external mod services behind one port. `mod.rs` holds `SourceProvider` (whose `download_policy`
  defaults to `DownloadPolicy::Unlimited` for an `AccountTier`), `SourceCapabilities`, the download/update request
  and result types, the `CollectionSource` port and the registry. `providers/testing.rs` is the contract every adapter must
  pass: it checks the id, the registry, the capabilities, the link schemes, the page URL, the sample deep link and the
  sample archive name, and fails when a registered provider has no case. `providers/download.rs` is the archive-download
  rule the adapters share (not Thunderstore's `download.rs`): it picks the archive's name and free target from the CDN
  header, the URL, the requested name and the reply type (`target_name`, `archive_name`, `archive_target`,
  `free_download_name`), maps an empty or HTML answer to the adapter's own error (`body_error`), and runs the links
  through `fetch_archive` with `ArchiveRules` carrying the provider's `download_policy`, pacer key and tier. Nexus file
  downloads, Thunderstore, Nexus collections, collection external links and pack links call it.
  `sources/downloads/` is the provider-neutral queue: links, state, progress, sidecar and auto-install; a provider only
  fetches the file. `providers/workers.rs` is the indexed worker runner both update checks share: one index per worker,
  answers in request order, a panicking worker reported instead of unwinding. `providers/thunderstore/`
  owns the Thunderstore adapter: `mod.rs`
  (the port: owner-name ids, the `ror2mm` scheme, the package's communities for a link that names no game), `link.rs` (the
  `ror2mm://v1/install/thunderstore.io/<owner>/<name>/<version>/` links and the name rules), `api.rs` (the experimental
  package API, read as JSON, and the archive address), `meta.rs` (page data, requirements and approved communities) and
  `download.rs` (the archive in the downloads folder, with its sidecar) and `collections.rs` (modpacks: the page link,
  the dependency list as the collection plan, and the version packed into a revision). `providers/nexus/` owns the whole Nexus adapter: `mod.rs`
  maps the port (key account, page URL, deep link, metadata, download, update check), and its submodules are the client
  and engine (`api/`, `download/` with `file.rs` and `target.rs`, `types.rs` for the Nexus records,
  `update/` with the page and requirement reads, `files/` for file groups and their `mod_file_id` parts
  (`mod.rs`, `parse.rs`, `order.rs`, `matching.rs`),
  `requirements.rs`, `page.rs`, `key.rs`, `link.rs`, `text.rs`, `collections/` for the collection revision, archive
  link and file page). A provider's saved key is read through
  `providers::saved_key` and `providers::required_key`, which give every caller the same `source.key_missing` error. `providers/picture.rs` holds the page pictures both
  providers share: a picture is saved once per
  mod in the game's `images/` folder, named by provider and id (`nexus-<id>`, `thunderstore-<owner>-<name>`), and read
  back as a data URL. The shared `<archive>.source.json` sidecar lives in `infra/sidecar.rs`; the family source id
  edit lives in `library/source_id.rs`. A new service (for example mod.io) is a new folder here, not a branch in
  `library`, `profiles`, `deploy` or `packs`; the game lists it in `providers` in `game.toml`.
- `test_support.rs`: `scratch!`, `write`, zip helpers, builders for profile rows, entries, versions, installed mods and
  deployment targets, and the synthetic game definitions for tests.
- `sources/requirements/`: the requirement planner split into `model.rs` (the plan view types), `plan.rs` (the closure
  and alternatives) and `install.rs` (the one plan's downloads and enable steps). The old `requirements.rs` was a single
  file; the public entry points stay in `mod.rs`.
