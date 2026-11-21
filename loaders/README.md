# Loader presets

A loader preset is the shared layout of a mod loader: the files it needs, the folders its mods use and the folders an
archive may ship at its root. It lives in `loaders/<id>.toml` and is data: the code never branches on a loader id.

A game takes a preset with one line in `game.toml`:

```toml
loader = "bepinex"                              # the preset's default_variant
loader = { id = "bepinex", variant = "6" }      # a named variant
```

The game keeps its own `detect`, `executables`, `providers`, `user_data`, `tools`, `steps`, `load_order` and
`deploy.root`: those belong to the game, not to the loader. A preset can only add `requires`, `requirements` and
`[deploy]` rules.

## File shape

```toml
schema = 1
id = "bepinex"
name = "BepInEx"
default_variant = "5"

# A requirement implies its path: winhttp.dll is required without a `requires` line.
[[requirements]]
name = "BepInEx"
path = "winhttp.dll"
url = "https://github.com/BepInEx/BepInEx/releases"

[deploy]
root_files = ["winhttp.dll", "doorstop_config.ini"]

[deploy.aliases]
plugins = "BepInEx/plugins"
core = "BepInEx/core"
config = "BepInEx/config"
patchers = "BepInEx/patchers"
monomod = "BepInEx/monomod"

[[deploy.slots]]
dir = "BepInEx/plugins"
root_extensions = [".dll"]
root_folder_extensions = [".dll"]

[[deploy.mirror]]
dir = "BepInEx/core"

[[deploy.mirror]]
dir = "BepInEx/config"

[[deploy.mirror]]
dir = "BepInEx/patchers"

[[deploy.mirror]]
dir = "BepInEx/monomod"

[variants."5"]
[[variants."5".requirements]]
name = "BepInEx 5"
path = "BepInEx/core/BepInEx.dll"
url = "https://github.com/BepInEx/BepInEx/releases"

[variants."6"]
[[variants."6".deploy.mirror]]
dir = "dotnet"
```

An archive may use the loader's own folder (`BepInEx/plugins/...`) or the folder alone (`plugins/...`). The preset lists
the second form in `[deploy.aliases]`; a folder that is itself a deploy place (`BepInEx/...`) is not a wrapper, so the
first form keeps its path.

The file name must match `id`. The id is a slug: lowercase letters, digits and `-`. `name` is the display name the
interface shows, for example `BepInEx`. A variant name is lowercase letters, digits, `-`, `_` or `.`.

`min_gobbo = "0.3.0"` names the oldest Gobbo release that serves the preset correctly, as in `game.toml`; every game
that names the preset inherits it.

`default_variant` is required when the preset has variants, so `loader = "id"` always means one version. It is an error
to name a default when there are no variants.

A preset may use every `[deploy]` key a game may use, except `root` and `root_linux`. The game owns the deployment
root; the loader describes the layout below it. This lets a game whose mods live outside its folder use the same preset.

A loader may not set `detect`, `executables`, `providers`, `user_data`, `profile_data`, `tools`, `steps` or
`load_order`. Unknown keys are errors.

## How the rules are merged

The order is:

```text
loader base  ->  loader variant  ->  game.toml
```

| Field                       | Merge                                                                                                                           |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `requires`                  | Added, no duplicates (case-insensitive path key). Loader first, then the game. Every requirement path is added too.             |
| `requirements`              | Keyed by `path`, case-insensitive. A higher layer replaces the name and URL; new entries are appended. Each `path` is required. |
| `deploy.root_files`         | Added, no duplicates. Loader first, then the game.                                                                              |
| `deploy.slots`              | Keyed by `dir`, case-insensitive. A higher layer replaces the whole slot. New slots are appended.                               |
| `deploy.mirror`             | Keyed by `dir`, case-insensitive. A higher layer replaces the whole mirror. New mirrors are appended.                           |
| `deploy.aliases`            | Keyed by alias name, case-insensitive. A higher layer replaces the target.                                                      |
| `deploy.protected`          | Added, no duplicates.                                                                                                           |
| `deploy.shared`             | Appended: loader entries first, then the game's.                                                                                |
| `deploy.root`, `root_linux` | Only the game. A loader that sets them is invalid.                                                                              |

A requirement implies its file: a file in `requirements` is required without a matching `requires` entry. Use `requires`
only for a file with no name or download page to show. A game's own requirements are implied the same way.

A game can add rules, override an entry with the same key, or replace a slot/mirror with one of the same kind. It
cannot remove a rule in this version. When a loader version removes or changes a rule incompatibly, make a new preset
file, for example `bepinex-7.toml`, or a new variant that does not inherit the problem.

A folder may not be a slot in one layer and a mirror in another. The merged definition is checked exactly like a game
that wrote every rule itself.

## Adding a loader

1. Create `loaders/<id>.toml` with `schema = 1`, the matching `id`, and the shared rules.
2. Add a test case that loads every variant, or rely on the existing test that walks every file here and merges each
   variant into a minimal game.
3. Point a game at it with `loader = "<id>"` and remove the rules that now live in the preset.
4. Run `cargo test` in `src-tauri`.

The test `every_shipped_loader_and_variant_merges_into_a_valid_game` walks this folder, so a new preset is checked
without a new test. A preset is not a game: it has no icon and no detection.

## Unreal and future loaders

A loader preset is not tied to Unity. A hypothetical UE4SS preset can be data too:

```toml
schema = 1
id = "ue4ss"
default_variant = "3"

requires = ["dwmapi.dll"]

[deploy]
root_files = ["dwmapi.dll"]

[[deploy.slots]]
dir = "Content/Paks/~mods"
extensions = [".pak", ".ucas", ".utoc"]

[[deploy.mirror]]
dir = "Binaries/Win64"

[variants."3"]
requires = ["ue4ss/UE4SS.dll"]
```

If Unreal 6 changes the folders, add a variant or a new file. If it needs a primitive the schema cannot express (a
tool, a load-order file, a step), that belongs to the game definition or to a future schema extension, not to a branch
in Rust.

## Limits

- One loader per game. A preset may contain several slots and mirrors, so it can describe more than one layout.
- No auto-detection of the installed loader version. The game definition chooses the variant; the definition editor
  suggests a variant when it finds the preset's files in the game folder.
- A user loader lives in `<data>/definitions/loaders/<id>.toml` and wins over a bundled or catalog-downloaded preset
  with the same id. The definition editor lists all three. Shipped and downloaded loaders stay read-only.
- A preset ships in the definition catalog with the games: an install brings the loader a game names when it is
  missing locally or out of date. See the catalog section in [games/README.md](../games/README.md).
- An exported game definition includes the user loader it names under `loaders/`. Importing the package installs both,
  so a shared game works even when the receiver does not have that loader yet.
- Editing a loader in a development build needs an app restart, because the game definition cache is keyed by
  `game.toml`.
