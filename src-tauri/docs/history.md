# Known exceptions and layout history

## Known exceptions

These do not follow the layer table yet. Do not add more of them; remove them when you touch the code next.

- The IPC helpers in `commands/` (`blocking`, `for_game`, `change`) are used by the command files of the modules, and
  `change` reads `status`. No two modules depend on each other in a cycle, but a module's command file reaches the IPC
  layer through these helpers. Move `blocking` and `for_game` below the modules, and `change` next to `status`, when you
  next touch them.
- `sources/update` is the provider-neutral update feature. The Nexus engine lives in `providers/nexus/update/`; a new
  provider implements `SourceProvider::check_updates` and the feature stores its generic result.
- `import/reinstall.rs` deliberately keeps the existing folder and its index record, so it unpacks through
  `install_zip_named_with_roots` with `Naming::only` instead of going through `acquire`/`import_batch` like other
  imports. A "keep existing naming" mode in the shared import would remove this exception, if one is ever needed.

The layout above replaced a flat `src/` with about 20 top-level modules, the `service` module that re-exported all of its
children, the process-wide game definition cache, and one error type per module with its own `io_error` helper. When you
need the reasons, read the commit messages from `refactor:` onward.
