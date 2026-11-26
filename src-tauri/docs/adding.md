# Adding a provider or a feature

### Adding a provider

A provider is an adapter, not a branch. The rest of the app sees `SourceRef`, `SourceMeta` and
`SourceRequirement`; it never names the service.

1. Create `providers/<id>/` with a `SourceProvider` implementation (the trait is in `providers/port.rs`). Keep the
   service's records inside that folder and map them to the generic types in `domain::source`.
2. Register it in `PROVIDERS` in `providers/registry.rs`. Add a `Case` to `providers/testing.rs` with a public mod id, page
   URL, deep link and archive name; `check_all` fails until every registered provider has one.
3. Use the shared pieces: `infra::http` for requests (`GOBBO_HTTP_TRACE=1` prints every request with keys and signed
   links redacted), `sources::downloads` for the queue and state, `infra::sidecar` for the `<archive>.source.json`
   file, and `providers::picture` for a saved page picture. The provider only fetches the file and maps its own errors.
4. Add every error code to `src/locales/en.ts` and `src/locales/pl.ts`. `src/locales/error-codes.test.ts` checks the
   literal `AppError::new` codes; a code built in a match arm is checked by hand.
5. Add the link schemes to `plugins.deep-link.desktop.schemes` in `tauri.conf.json`; the test in `app/mod.rs` fails
   when the config and the registry disagree.
6. `check_updates` returns exactly one `SourceUpdate` per request, in the request order, and reports a failure in that
   update instead of dropping it. A download writes its sidecar through `infra::sidecar` and goes through the shared
   queue, so the window's download list and auto-install work without provider code.
7. No provider name in `library`, `profiles`, `deploy`, `packs` or `status`. When a feature needs a provider value, add
   it to the generic model or pass it in.
8. A provider with paid tiers reports the account's access through `SourceAccount::direct_download` in `validate_key`.
   The window uses that flag, never the provider id, to choose a direct update or the provider's files page.
9. Set `download_policy` for an account tier when the service paces its downloads: the port defaults to
   `DownloadPolicy::Unlimited`, and Nexus overrides it for free accounts. The shared queue reads the policy, so the
   rule stays on the provider instead of in the download code.

### Adding a store

A store is an adapter, not a branch in detection.

1. Implement `StoreAdapter` in `games/stores/mod.rs` (or a submodule for a store with several readers) and add one entry
   to the static `ADAPTERS` list in scan order. `candidates_in` picks it up; the scan itself does not change.
2. Add the store to the `stores!` list in `games/stores/mod.rs` so `name`, `display_name`, `ALL` and `from_name` stay in
   step, and to `platform::store_folders` for the folder it keeps games in. Steam keeps its own detection and `Manual`
   is the fallback, so neither is an adapter.
3. Add the store's id list to the definition's detect fields in `games/definition/` and its `IdKind` in
   `games/stores/mod.rs` when a candidate carries an id: a folder with the right markers is never taken for a game that
   did not opt into it.

### Adding a feature

A feature is a vertical module: its commands, use case and engine live in one folder, and the shared layers
(`domain`, `infra`, `context`, `providers`) stay untouched.

1. Pick the module that owns the data. A new game is data; a new provider is a folder in `providers/`; everything
   else starts in `library/`, `deploy/`, `import/`, `launch/`, `profiles/`, `packs/`, `sources/`, `status/` or `mods/`.
2. Write the use case there as a public function. Read through `context`, never through another feature's private code.
3. Add a command in that module's `commands.rs` with `game_commands!` when it runs for one game; register its
   `alias::name` in `src-tauri/commands.txt` and add its `allow-*` to `capabilities/default.json`. The single list in
   `commands.txt` generates both the handler registration and the permissions.
4. If the window needs new data, add a view type in `status/views/` and fill it in `status/mod_builders/` or `status/`
   instead of sending a second call.
5. Add the strings to `src/locales/en.ts` and `src/locales/pl.ts`; an error code without a translation shows the code.
6. Add the test next to the use case. A shared fixture goes in `test_support.rs`; a cross-module flow goes in
   `src/tests/`.
7. Run the gates, regenerate `src/bindings.ts` when a command or IPC type changed, and update `AGENTS.md` plus
   `CHANGELOG.md` when the structure or the user-visible behavior changed.

### API design

- For new public APIs, prefer a newtype or an enum to a `bool` or an `Option` parameter (Rust API Guidelines C-NEWTYPE
  and C-CUSTOM-TYPE). Existing code does not need to change for this rule.
- Keep constructors as `new` or `from_*` functions; keep fields private unless the type is a plain record.
