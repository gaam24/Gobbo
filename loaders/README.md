# Loader presets

A loader preset is the shared layout of a mod loader: the files it needs, the folders its mods use and the folders an
archive may ship at its root. It lives in `loaders/<id>.toml` and is data: the code never branches on a loader id.

A game takes a preset with one line in `game.toml`:

```toml
loader = "bepinex"                              # the preset's default_variant
loader = { id = "bepinex", variant = "6" }      # a named variant
```

The game keeps its own `detect`, `executables`, `providers`, `user_data`, `tools`, `steps`, `load_order` and
`deploy.root`: those belong to the game, not to the loader. A preset can only add `requires`, `requirements`,
`install` and `[deploy]` rules; it may also set `config_mirror` on the preset or on a variant.

## File shape

```toml
schema = 1
id = "bepinex"
name = "BepInEx"
config_mirror = "BepInEx/config"
min_gobbo = "0.2.0"
default_variant = "5"

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

[variants."5".install]
name = "BepInEx"
version = "5.4.23.6"
to = ""
check = ["winhttp.dll", "BepInEx/core/BepInEx.dll"]
url = "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.6/BepInEx_win_x64_5.4.23.6.zip"
sha256 = "6c57a714a892efb4e9e1fef63d608a3391f30ec984031c465404e4ee381a0755"
size = 642076

[variants."6"]
[[variants."6".requirements]]
name = "BepInEx 6"
path = "winhttp.dll"
url = "https://github.com/BepInEx/BepInEx/releases"

[[variants."6".deploy.mirror]]
dir = "dotnet"
```

An archive may use the loader's own folder (`BepInEx/plugins/...`) or the folder alone (`plugins/...`). The preset lists
the second form in `[deploy.aliases]`; a folder that is itself a deploy place (`BepInEx/...`) is not a wrapper, so the
first form keeps its path.

The file name must match `id`. The id is a slug: lowercase letters, digits and `-`. `name` is the display name the
interface shows, for example `BepInEx`. A variant name is lowercase letters, digits, `-`, `_` or `.`.

`config_mirror` names the game-relative folder where an unpacked community profile keeps this loader's config files,
when it has one. The community-profile import checks it together with the bare `config` folder that any unpacked profile
may use. A variant may name its own folder, which replaces the preset's value for that variant. The path must stay inside
the game and name a folder the preset declares as a slot, a mirror or an alias target; a typo then fails when the preset
loads instead of silently missing the profile's config files. A preset without config files leaves the field out; no
loader id is named in code.

`min_gobbo = "0.2.0"` names the oldest Gobbo release that serves the preset correctly, as in `game.toml`; every game
that names the preset inherits it.

`default_variant` is required when the preset has variants, so `loader = "id"` always means one version. It is an error
to name a default when there are no variants.

A preset may use every `[deploy]` key a game may use, except `root` and `root_linux`. The game owns the deployment
root; the loader describes the layout below it. This lets a game whose mods live outside its folder use the same preset.

A loader may not set `detect`, `executables`, `providers`, `user_data`, `profile_data`, `tools`, `steps` or
`load_order` at its top level. A variant may set `load_order`, the order file its release reads (a variant names it
because each release keeps it in its own folder); a game's own `load_order` wins over it. Unknown keys are errors.

## How the rules are merged

The order is:

```text
loader base  ->  loader variant  ->  game.toml
```

| Field                       | Merge                                                                                                                           |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `requires`                  | Added, no duplicates (case-insensitive path key). Loader first, then the game. Every requirement path is added too.             |
| `requirements`              | Keyed by `path`, case-insensitive. A higher layer replaces the name and URL; new entries are appended. Each `path` is required. |
| `install`                   | One package. A higher layer appends check paths and `needed_by`; a name, version, `to` or source it sets replaces the lower one. |
| `deploy.root_files`         | Added, no duplicates. Loader first, then the game.                                                                              |
| `deploy.slots`              | Keyed by `dir`, case-insensitive. A higher layer replaces the whole slot. New slots are appended.                               |
| `deploy.mirror`             | Keyed by `dir`, case-insensitive. A higher layer replaces the whole mirror. New mirrors are appended.                           |
| `deploy.aliases`            | Keyed by alias name, case-insensitive. A higher layer replaces the target.                                                      |
| `deploy.protected`          | Added, no duplicates.                                                                                                           |
| `deploy.shared`             | Appended: loader entries first, then the game's.                                                                                |
| `deploy.moves`              | Appended: loader entries first, then the game's.                                                                                |
| `deploy.root`, `root_linux` | Only the game. A loader that sets them is invalid.                                                                              |
| `config_mirror`             | A variant's value replaces the preset's; one that is absent keeps the preset's. Never merged from the game.                     |

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
without a new test. A preset is not a game: it has no icon, and its `[engine]` table (see below) only suggests it in the
editor.

## Engine detection

A preset can describe the engine it serves, so the add-a-game editor recognises a game folder that has no loader yet.
The `[engine]` table names the markers that must all exist in the game folder, and the variant that fits:

```toml
[engine]
name = "Unity"
markers = ["UnityPlayer.dll|UnityPlayer.so", "*_Data"]   # all must exist in the game folder
[[engine.variants]]                               # the first whose markers all exist wins
variant = "6"
label = "IL2CPP"
markers = ["GameAssembly.dll"]
[[engine.variants]]
variant = "5"
label = "Mono"
markers = ["*_Data/Managed"]
```

- `name` is the engine's display name. Every marker is a path with `/` separators. A `*` in one segment matches any run
  of characters in that segment, so `*_Data` matches `Example_Data` and `*` matches any folder name. Case is ignored as
  every path key ignores it, on Windows and Linux alike.
- A marker may list alternatives with `|`: `UnityPlayer.dll|UnityPlayer.so` is satisfied when either file is there. Use
  it when one file has another name in another build, such as a native Linux build of a Windows game. Each alternative
  is a folder pattern of its own (spaces around `|` are ignored), and an empty alternative is an error. The same rule
  applies to a variant's markers; `root` takes one pattern only.
- `variant` is suggested when no `[[engine.variants]]` entry matches; without it the preset's `default_variant` is
  suggested. A variant named here must exist in the preset. Put a variant whose markers overlap another one first.
- `root` is an optional `*/` pattern for the project folder, for example `root = "*/Content/Paks"`. The folder its first
  segment matches becomes the game's deployment root, `{game}/<folder>`, when the draft has no root yet and its loader is
  this preset. A folder that a marker names (`Engine` for `Engine/Binaries`) is the engine's own and is never the project.
- A folder can match several presets. The one with the most matching markers is suggested; presets are checked in id
  order, so a tie keeps the earlier one.

An installed loader stays the stronger signal: the editor keeps the loader it finds in the folder and still reports the
engine. The engine suggests a loader only when the draft has no layout yet. The check reads the first levels the patterns
name and never writes. The table is a suggestion, not a game rule: a game takes nothing from it, and its own
`loader = "..."` line is the choice that counts.

The shipped `bepinex` preset names Unity (IL2CPP first, then Mono, with 5 as the default), and `unreal` names Unreal
Engine with the UE4SS variant as the default.

## Unreal Engine

`loaders/unreal.toml` is the shared layout for Unreal Engine 4 and 5. Its rules are relative to the game's **project
folder** — the folder that holds `Content/` and `Binaries/`, for example `Pal` in Palworld or `Phoenix` in Hogwarts
Legacy. The game sets that folder, because only the game definition knows its name:

```toml
[deploy]
root = "{game}/Pal"
```

The preset has layout variants. `pak` is the default and carries the flat `Content/Paks/~mods` slot for `.pak` and
IoStore files, the `Content/Paks/LogicMods` mirror and their archive aliases. `ue4ss-4` and `ue4ss-3` add the
UE4SS mod slot of the matching release line:

```toml
loader = { id = "unreal", variant = "ue4ss-4" }
```

A UE4SS mod is a folder with `Scripts/main.lua` (Lua) or `dlls/main.dll` (C++). The slot lists both in `markers` and
writes `enabled.txt`, which starts the mod. It does not order it. The order is `mods.txt`, the list in the variant's
`load_order` (`Binaries/Win64/ue4ss/Mods/mods.txt` for `ue4ss-4`, `Binaries/Win64/Mods/mods.txt` for `ue4ss-3`).

The mod list window gives the mods the user orders a number, and Gobbo writes them into that file as `Folder : 1`
(enabled) or `Folder : 0` (off), in the order of the numbers. The file lives in the deployment root, so it is written
into the profile's mod layer and the game folder stays untouched. The lines the release ships (for example
`BPModLoaderMod : 1`) keep their place; an ordered mod takes the place of its own first line, or goes at the end. Blueprint
paks in `LogicMods` still need UE4SS's `BPModLoaderMod` enabled in that list.

The preset also carries the UE4SS package itself:

```toml
[variants."ue4ss-4".install]
name = "UE4SS"
version = "4.0.0"
to = "Binaries/Win64"
check = ["Binaries/Win64/dwmapi.dll", "Binaries/Win64/ue4ss/UE4SS.dll"]
needed_by = ["Binaries/Win64/ue4ss/Mods", "Content/Paks/LogicMods"]
url = "https://github.com/UE4SS-RE/RE-UE4SS/releases/download/v4.0.0/UE4SS_v4.0.0.zip"
sha256 = "..."
size = 8758673
```

The loader is a mod, exactly like a BepInEx pack: it lives in the library and reaches the game through the overlay, and
**nothing is written into the game folder**. The importer recognises a loader release: an archive that holds a file
named like every program among the `check` paths (`.dll`, `.exe`). The folder that holds the first of those programs is
the release's root, and every file below it is placed at the same path under `to`, so the official zip, a repack under
`Pal/Binaries/Win64/` and a build with or without extra files all become one mod at game paths (`Binaries/Win64/
dwmapi.dll`, `Binaries/Win64/ue4ss/...`, the bundled UE4SS mods included). Under Proton the proxy is loaded native
like every other loader DLL that is there. UE4SS's settings and logs written while the game runs land in the profile's
overwrite folder, so an update of the loader keeps them.

`needed_by` names the folders whose mods need the package. Once an enabled mod puts a file there, the mod list shows an
**Install UE4SS** button (the header's primary action, a button in the problems block and a step in Getting started)
and the apply waits until an enabled mod, or a manual install in the game folder, provides every `check` path. A list
of plain paks never asks for UE4SS. An empty `needed_by` means every mod needs the package, as BepInEx would.

**Install** runs as a task in the Working panel. A copy already in the library is turned on as it is; otherwise Gobbo
downloads the release, stops a download that grows past `size`, checks the SHA-256, imports it, names it after the
package with its version, and turns it on in the active profile. Dropping the release onto the window imports it the
same way; it joins the list turned off, and the install button turns it on. The setup check shows the name, version,
source, hash, size and the copy in the library, and offers an update when the definition names a newer version.

A game's own `[install]` adds check paths and `needed_by` folders, and may leave `name` and `version` out. When it names
its own build with `url`, `sha256` and `size`, that build replaces the preset's: Palworld installs Okaetsu's Palworld
build of UE4SS, which ships the game's `MemberVariableLayout.ini`, while the preset's `to`, check paths and `needed_by`
folders still apply. A game can have one package. The package is data; BepInEx 5 uses the same mechanism.

A game with no install block keeps the older manual flow: a `[[requirements]]` entry names the file and links its page.

## Loader packages are offered, never forced

The package in `install` is the official release, but it is only one copy of the loader. Mod sites often publish a
build of the loader for one game — a newer release, a pack with the game's fixes, or a bleeding-edge build — and mods on
that site expect it. Gobbo treats every mod in the library that ships each `check` file as a copy of the loader:

- The apply needs **an** enabled copy, whichever it is. Any enabled mod that provides the `check` files, or a manual
  install in the deployment root, meets the package.
- The setup check lists the copies with their name and version, marks the one in use and the official release (the copy
  unpacked from the archive with the definition's SHA-256), and offers **Use this one** for each other copy and a
  button for the official release. The one the user picks becomes the only enabled copy in the active profile; the
  others stay in the library, turned off, so two loaders never fight over the same proxy.
- The quick action (the header's **Install** button, the problems block and Getting started) turns on a copy that is
  already in the library, the official release first and then the newest import, and downloads the official release
  only when the library has none.
- **Update available** is shown only for an older copy the official install put there. A build from a mod site is the
  user's choice and is never reported as outdated.

`to = ""` (or no `to`) means the release's root is the deployment root itself, as for BepInEx, whose proxy sits next to
the game's program. When the game's own rules already put every `check` file at its game path, the importer keeps them:
a BepInEx pack that bundles plugins still sends the plugins to their slot. Only when the rules cannot place the release
(UE4SS below the program folder) does the release become one mod at game paths below the folder that holds its first
program, so a repack wrapped in more folders lands where the original does and files outside it are skipped.

## BepInEx

`loaders/bepinex.toml` is the shared layout for Unity games. Variant `5` (Mono, the default) carries the official
64-bit Windows release as its package, with `winhttp.dll` and `BepInEx/core/BepInEx.dll` as check files and an empty
`needed_by`, so any enabled mod asks for the loader. Variant `6` (IL2CPP) names no package: BepInEx 6 is a pre-release
whose core file names change between builds and IL2CPP games usually need a bleeding-edge build, so it requires the
proxy and links the release page.

BepInEx has no load-order file: a plugin declares the plugins it needs and BepInEx sorts them. The mod list order only
decides which mod wins a file both ship. A 32-bit game, or a native Linux build that loads BepInEx through
`run_bepinex.sh`, needs its own `[install]` with the matching release.

A pak-only game can name `loader = "unreal"` and gets only the shared pak rules. UE4SS is a 64-bit Windows loader, so a
32-bit game can still use the `pak` variant but not the `ue4ss-*` variants. A game whose bare paks live in
`Content/Paks` instead of `~mods` adds its own `move` rule, or a game with a different tree uses its own definition; a
game on Xbox/Game Pass uses a different binary folder and needs its own variant. Pak load order is the file name and is
not managed yet, and the `.pak`/`.utoc`/`.ucas` group of one mod can be split only if two mods ship the same file
names — both are known limits, not per-game branches.

If Unreal 6 changes the folders, add a variant or a new file. If it needs a primitive the schema cannot express (a
tool, a load-order file, a step), that belongs to the game definition or to a future schema extension, not to a branch
in Rust.

## Limits

- One loader per game. A preset may contain several slots and mirrors, so it can describe more than one layout.
- No auto-detection of the installed loader version. The game definition chooses the variant; the definition editor
  suggests a variant when it finds the preset's files in the game folder, and the `[engine]` table suggests one when a
  game folder has no loader yet.
- A user loader lives in `<data>/definitions/loaders/<id>.toml` and wins over a bundled or catalog-downloaded preset
  with the same id. The definition editor lists all three. Shipped and downloaded loaders stay read-only.
- A preset ships in the definition catalog with the games: an install brings the loader a game names when it is
  missing locally or out of date. See the catalog section in [games/README.md](../games/README.md).
- An exported game definition includes the user loader it names under `loaders/`. Importing the package installs both,
  so a shared game works even when the receiver does not have that loader yet.
- Editing a loader in a development build needs an app restart, because the game definition cache is keyed by
  `game.toml`.
