# Game definitions

One folder per supported game: `games/<id>/game.toml`, and optionally `icon.webp` (see [Icon standard](#icon-standard)).
A definition is data: the code never branches on a game id or a loader id. Every rule the code relies on is checked
when the file loads, so a wrong definition fails with a message that names the file and the problem.

A game ships after it has been tested on a real installation.

## Add a game

A game can also be added from the window: **Games → Add your own game** reads a game folder, suggests the definition
and writes it under `<data>/definitions/games/<id>/`. The same editor edits a user definition again and checks a draft
against a real mod archive without importing it. The editor also reads the folder for its engine: a loader preset's
`[engine]` table names the markers of an engine, and the editor preselects the loader and variant that fit (see
[loaders/README.md](../loaders/README.md#engine-detection)). The rest of this section is the file format the editor
writes.

1. Copy `_template/` to `games/<id>/` and fill in `game.toml`. Delete what does not apply.
2. Add `icon.webp` (optional; see [Icon standard](#icon-standard)). Without it the interface falls back to the first
   letter of the name.
3. If the game uses a known loader (BepInEx, MelonLoader, UE4SS), add `loader = "<id>"` and leave the shared
   `requires`, `requirements` and `[deploy]` rules to `loaders/<id>.toml` (see [Loader presets](#loader-presets)).
   Run `cargo test --lib games` in `src-tauri` for a focused check, or the full `bun run check:rust` from the root.
   The tests read every folder here and every loader preset, so nothing else changes in the code. A Unity
   game with the BepInEx slot needs nothing more: `src-tauri/src/tests/unity.rs` runs the same import, deployment and
   launch checks on every shipped definition that has that slot. Tests never name a game.
4. Test on a real installation: detection, import, apply, undo, launch.
5. The two supported-games tables (this file and the root `README.md`) are checked against the `games/*` folders by
   `bun run guard`: the folder name and the top-level `name` in `game.toml` are the source, and adding a game
   cannot update one list without the other. The README summary is checked against the tested counts here too.
6. If the game needs a rule the schema cannot express, add a reusable primitive with tests. Never a branch for one game.

### Game id

`id` is the full title of the game as a slug: lower case ASCII, digits and `-`, and the same as the folder name. Keep
the words of the title, drop punctuation and accents, and separate words with `-`:
`the-witcher-3-wild-hunt`, `cyberpunk-2077`, `lethal-company`, `tainted-grail-the-fall-of-avalon`. `name` carries the
title with its punctuation and capitals for the interface.

The provider keys are separate: Nexus calls The Witcher 3 `witcher3` and Thunderstore calls Lethal Company
`lethal-company`; those stay in `providers`, not in `id`. When two games share a title, add the edition, year or
subtitle to the id. Once shipped, an id is stable: a later rename needs a migration of the per-game data folder
(`games/<id>` and `games/<id>.removed`) and an entry in `src-tauri/src/migrations/game_ids.rs`, not just a folder rename.

## Icon standard

A game icon is optional. When present it is one file, `games/<id>/icon.webp`:

- **128×128 px, 1:1.** Crop to a square; never stretch. Trim a transparent border before cropping.
- **WebP, lossy, quality 85**, alpha quality 100, method 6, sharp YUV, sRGB, no metadata. Keep transparency transparent;
  do not flatten it onto a colour.
- **At most 8 KB**; 12 KB is the hard limit. A larger file is a failed icon, not a reason to change the format.
- Use the largest source on SteamGridDB (512×512 or 1024×1024), not a 16/32/64 download. An ICO is accepted as a source;
  the largest frame is used. `icon.png` is a legacy fallback only; the shipped definitions use WebP.

`bun scripts/icons.ts` normalizes every `games/*/icon.webp` in place and leaves files that already meet the standard
alone. `--source <dir>` reads `<dir>/<game-id>.{png,webp,jpg,jpeg,ico}`, `--map <file.json>` maps a game id to a file
when the download name is not the game id, and `--check` verifies the set. The script needs `bun install` for `sharp`.

## How mods reach a game

A game has a deployment root (the game folder by default) and three ways to place files under it. A game may use
slots and mirrors together; `full_root` is used instead of them.

- **Slot** (`[[deploy.slots]]`): a folder where each mod gets a folder of its own, and the game loads every folder:
  `Mods/<mod>/...`, `BepInEx/plugins/<mod>/...`. A flat slot takes the files directly: `Content/Paks/~mods/*.pak`.
- **Mirror** (`[[deploy.mirror]]`): a game folder (or one file) whose files keep the paths they have in the archive:
  `archive/pc/mod/...`, `DLC/<dlc>/...`, `Data/...`, `bin/x64/version.dll`.
- **Full root** (`full_root = true`): every archive path is a game path, so a new loader or framework needs no entry.
  The overlay covers the game's own files; a bare file at the archive root still needs `root_files` or a `move` rule.

Two mods that write the same game path conflict. The higher one on the list wins, unless the user picks the winner for
that path; only the winner's copy is deployed.

Every game uses the same plan. The overlay (overlayfs on Linux, a hook DLL on Windows) shows the mods through a layer:
the game's own folder never changes, so a mod may also cover a file of the game, and each profile keeps its own layer.
There is no method that writes the game folder.

### How an archive is read

Each file of an archive is placed by the first rule that takes it. Anything no rule takes (a readme, for example) is
skipped. A single folder that wraps the game folders (`ModName/Mods/...`) is taken off first; a folder that is itself a
game folder (`BepInEx/...`) is not a wrapper. Then a first folder that `[deploy.aliases]` names is replaced by the game
folder it stands for (`plugins/<Mod>/...` becomes `BepInEx/plugins/<Mod>/...`), so the rules below see the game path.

1. A file under a mirrored folder keeps its game path, and so does a bare file at the archive root that `root_files` names.
2. A bare file at the archive root that a `[[deploy.move]]` rule takes goes to that rule's folder, keeping its name
   (`loose.archive` becomes `archive/pc/mod/loose.archive`).
3. A file under a mod root that a slot's `marker` or `markers` finds belongs to that mod: `<modlet>/ModInfo.xml` makes
   `<modlet>` a mod; a marker at the archive root makes the whole archive one mod. A folder at the archive root that
   holds a file of a slot's `root_folder_extensions` is a mod too: `<Plugin>/<plugin>.dll` makes `<Plugin>` one. In a
   flat slot the folder is only the library name; its files deploy by file name into the slot folder.
4. A file under a slot folder: `Mods/<mod>/...` is the mod `<mod>`, and a file directly in the slot folder belongs to
   the archive's mod.
5. A file in a `protected` folder that no rule took refuses the whole archive, so an archive cannot silently place a
   file in a folder the game owns; a mirror has to name it explicitly.
6. A bare file at the archive root goes to the first slot whose `root_extensions` include it.
7. With `full_root = true`, any remaining path with a folder is a game path. A bare file at the archive root still needs
   `root_files` or a `move` rule.

Mirrored files join the archive's mod in the library, so one archive stays one mod (a `Mods/` folder and its `DLC/`
folder are deployed and removed together). One mod keeps the archive's name; an archive with several mods gets one
library folder per mod.

## Loader presets

A loader used by several games lives once in `loaders/<id>.toml`. A game names it with one line:

```toml
loader = "bepinex"                              # the preset's default variant
loader = { id = "bepinex", variant = "6" }      # a named variant
```

The preset's `requires`, `requirements` and `[deploy]` rules are merged under the game's own: the game can add an entry, override one
with the same key, or omit `[deploy]` entirely when the preset already describes the whole layout. The game keeps its
own `detect`, `executables`, `providers`, `user_data`, `tools`, `steps`, `load_order` and `deploy.root`.

The loader never chooses the deployment root. A preset can contain several slots and mirrors, and one game has one
loader. The Game definition panel shows the preset's `name` and the variant it uses; a variant that comes from
`default_variant` is marked as the default. See [`loaders/README.md`](../loaders/README.md) for the file schema, the
merge rules, variants and versioning.

## Schema

Unknown keys are errors. Paths are relative to the game folder (or to the deployment root under `[deploy]`) and use
`/`. Extensions are lowercase with the dot. Overlay is the only deployment method.

Some keys documented below — verification, copy-only profile data, bare requirements, an alternate Linux root, extra
store ids, load-order bounds and launch `args`, including a loader's `requires` — are accepted for user-written and
older definitions but unused by the shipped catalog.

### Top level

| Key            | Meaning                                                                                                                                                                  |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `schema`       | `1`.                                                                                                                                                                     |
| `id`           | Lowercase letters, digits and `-`; the folder name.                                                                                                                      |
| `name`         | The name shown to the user.                                                                                                                                              |
| `min_gobbo`    | The oldest Gobbo release that serves this file correctly, such as `"0.3.0"`. Optional; see the catalog section.                                                          |
| `providers`    | Tables per external provider the game is on, for example `[providers.nexus]`. A provider without an entry is not offered.                                                |
| `loader`       | A shared preset from `loaders/<id>.toml`: a string for its default variant, or `{ id = "...", variant = "..." }`.                                                        |
| `requires`     | Files the game needs before mods work, with no display name or page. A mod can provide them. Requirement paths are required too.                                         |
| `requirements` | Files the game needs, with a display name and a download page. The setup check shows the name and offers **Get**. Each `path` is required; no `requires` line is needed. |
| `profile_data` | Path templates of files and folders kept per profile by copying: simple one-file entries. The order file is added on its own.                                            |
| `user_data`    | Folders the game keeps outside its own folder: saves, settings, config. Each profile can share or isolate one.                                                           |
| `logs`         | Logs the game itself writes, shown in the console after a launch. Mod `.log` files are discovered automatically.                                                         |

### `[providers.<id>]`

An external mod service the game is on. The table key is the provider id (`nexus`, `thunderstore`, `modio`, ...). `key` is
what that provider needs to address the game: the Nexus domain (`witcher3`), the Thunderstore community identifier
(`riskofrain2`) or the mod.io game id. A provider entry without a key is refused when the definition loads.

```toml
providers = { nexus = { key = "witcher3" } } # the game's part of its Nexus Mods address; enables update checks
providers = { thunderstore = { key = "riskofrain2" } } # the community; a ror2mm:// install link finds the game through it
```

A Thunderstore mod is named `owner-name` (for example `tristanmcpherson-R2API`). A `ror2mm://` link names the package but
not the game, so Gobbo takes the first community the package is listed in that the user has added as a game.

### `[[user_data]]`

A folder the game writes outside its own folder, for example `{documents}/The Witcher 3`. A profile either shares it
with the other profiles or keeps it separate. The definition carries no user-visible text: `kind` is a universal label
the frontend translates.

| Key       | Meaning                                                                                                                                                                                             |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`      | Lowercase letters, digits and `-`; unique in the game (`game` is reserved). It names the profile's state folder, not a UI label.                                                                    |
| `kind`    | `user-data` (default), `settings`, `saves`, `config` or `mods`. One shared translation per kind, for every game.                                                                                    |
| `path`    | A path template naming the folder, for example `{documents}/The Witcher 3`.                                                                                                                         |
| `path_linux` | A path template for a native Linux build. Empty or missing falls back to `path`.                                                                                                                 |
| `mode`    | `redirect` (default) serves it through the overlay; `copy` copies it around a launch. The app falls back to copy.                                                                                   |
| `default` | `shared` (default) or `isolated` for a new profile. Profiles created before the set use `shared`.                                                                                                   |
| `inherit` | `true` (default): an isolated redirect sees the existing folder as a read-only base. `false`: the first isolated launch copies the existing folder into the profile once, then the profile owns it. |

Rules:

- `redirect` covers a folder, not a single file. A single file belongs in `profile_data`.
- A set must name a folder outside the game folder; the game mount already covers anything inside it.
- Point at the folder the game itself lists as its user-data root. A nested redirect does not change the listing of its
  parent, so `.../The Witcher 3/gamesaves` would not appear in `.../The Witcher 3` when the real folder is missing.
- Two sets may not overlap, also when different templates resolve to the same folder. A game may declare at most 7
  sets; the game mount is the eighth mount of the session.
- A `profile_data` entry inside a `user_data` set is redundant; remove it. The set owns the whole subtree.
- The overlay is needed for a real redirect. Windows uses the hook; Linux uses the copy fallback, which is slower but
  works without a redirect.

### `[detect]`

| Key                  | Meaning                                                                           |
| -------------------- | --------------------------------------------------------------------------------- |
| `steam_app_ids`      | Steam app ids. The first one also finds the Proton prefix.                        |
| `steam_install_dirs` | Folder names under `steamapps/common`, tried when no app manifest names one.      |
| `gog_ids`            | GOG product ids (the base game, not a DLC). Matched by every way a GOG game is installed. |
| `epic_ids`           | Epic `AppName`s. Matched by every way an Epic game is installed.                  |
| `heroic_slugs`       | Extra Heroic app names, only for a Heroic app that has no GOG or Epic id.         |
| `lutris_slugs`       | Lutris game slugs, for a game Lutris installed from neither GOG nor Epic.         |
| `markers`            | Files that identify the game folder.                                              |
| `linux_markers`     | Files that identify the folder of the native Linux build. Used only on Linux.    |
| `any_marker`         | `true`: one marker is enough (builds with different file names). Default `false`. |

Where each id is read. A GOG or Epic id matches its game however it was installed, so a definition rarely needs
`heroic_slugs` or `lutris_slugs`.

| Store     | Read from                                                                                                               |
| --------- | ----------------------------------------------------------------------------------------------------------------------- |
| Steam     | `libraryfolders.vdf` and `appmanifest_<id>.acf` in every library                                                       |
| GOG       | the `goggame-<id>.info` files in each game folder (the game and each DLC), GOG's registry entries on Windows, and the folders of Galaxy, GOG's installers and minigalaxy |
| Epic      | the launcher's `.item` manifests (its data folder from the registry) and `LauncherInstalled.dat`; unfinished downloads and DLC are skipped |
| Heroic    | its Epic and GOG install lists, native and Flatpak, with the Wine prefix from `GamesConfig`                             |
| Legendary | its own `installed.json`, in `LEGENDARY_CONFIG_PATH`, `$XDG_CONFIG_HOME/legendary` or `~/.config/legendary`          |
| Lutris    | its database `pga.db`, native and Flatpak, with the Wine prefix from the game's file; a game Lutris installed from GOG or Epic also carries that id |

### `[[executables]]`

The programs the game starts with; the first one is the default. `name`, `path`, and `args` (always passed, before
the user's own arguments). `path_linux` names the program of a native Linux build; on Linux Gobbo starts it directly,
without Proton, and falls back to `path` when it is empty. There is no automatic anti-cheat detection: if a game needs
a launch flag, put it in `args`.

### `[deploy]`

These keys are the game's **mod layout**, not a choice of deployment method: overlay is the only method. They compose:
`slots`, `mirror` and `full_root` decide where files land; `shared`, `root_files`, `protected`, `aliases`, `move` and
`resource_names` refine that rule. A game usually needs one placement rule and at most one or two refinements.

| Key              | Meaning                                                                                                                                                                                                                              |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `root`           | The deployment root as a template, `{game}` by default. It may lie inside the game (`{game}/Data`) or outside it (`{appdata}/Game`); overlay covers either.                                                                          |
| `root_linux`     | The root of a native Linux install, which has no Proton prefix. Falls back to `root`.                                                                                                                                                |
| `root_files`     | Bare file names that keep their name in the deployment folder, for example a loader's proxy `winhttp.dll`. Overlay keeps them in the mod layer.                                                                                      |
| `resource_names` | `true`: the game resolves loose files by file name, so two files with the same name conflict even in different folders. Use only for a game with one global override tree.                                                           |
| `protected`      | Game folders an archive may place files into only through a mirror.                                                                                                                                                                  |     |
| `aliases`        | Archive folder names that stand for a game folder, as a table: `plugins = "BepInEx/plugins"`. The name is one folder, case is ignored, and the target lies inside a slot or mirror.                                                  |
| `full_root`      | `true`: the whole deployment root is the mod layout. Every archive path is a game path; a file the game already has is covered by the layer, and the game folder stays untouched. Bare files still need `root_files` or a move rule. |
| `move`           | Bare files at the archive root with these extensions move to `to`, keeping their name. It is the MO2-style rule for a game that loads loose archives from one folder.                                                                |

Overlay is the only deployment method. It covers the deployment root, including a root outside the game folder; there
is no fallback that writes the game folder. A `full_root` game needs no mirror for a loader or a framework: a new
folder is a new game path. A `[[deploy.move]]` entry is for a bare file the game reads from one known folder:

```toml
[[deploy.move]]
extensions = [".archive", ".xl"]
to = "archive/pc/mod"
```

`[[deploy.slots]]`:

| Key                      | Meaning                                                                                                                                                                                                 |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `dir`                    | The slot folder. It also names the slot in the library.                                                                                                                                                 |
| `prefix`                 | The start of the mod folder names in the game, for example `mod`.                                                                                                                                       |
| `marker`                 | A file or folder that marks a mod root in an archive, for example `ModInfo.xml`, `content`.                                                                                                             |
| `markers`                | More markers for a loader that accepts several layouts, for example `["Scripts", "dlls"]` for UE4SS Lua and C++ mods. A root found by two markers is added once.                                         |
| `enable_file`            | A file name Gobbo writes into every mod folder in the slot, for a loader that loads only a folder that has it. UE4SS reads `enabled.txt`. A flat slot cannot use it.                                      |
| `extensions`             | Only these files go into the slot.                                                                                                                                                                      |
| `root_extensions`        | Bare files at the archive root with these extensions go into the slot too.                                                                                                                              |
| `root_folder_extensions` | Folders at the archive root that hold a file with these extensions are mods: `<Plugin>/<plugin>.dll`. In a flat slot the folder names the mod in the library, but its files deploy directly into `dir`. Omitted, it copies `root_extensions`; an explicit empty list turns the rule off. |
| `flat`                   | The files go directly into `dir`, without a folder per mod.                                                                                                                                             |

`[[deploy.mirror]]`: `dir` (a folder or a single file), `extensions` (only these files), `prefix` (a folder directly
under `dir` gets it when it lacks it: `DLC/Corvo` becomes `DLC/dlcCorvo`).

`[[deploy.shared]]`: files that each mod keeps in its own folder, but that the game reads from one mod only (The Witcher
3's scripts). `dir` (inside the mod, empty means anywhere) and `extensions`. Two mods with the same path conflict.

### `[load_order]`

The file the game reads its mod order from. Without this table the game orders its mods itself, and the list decides
only the conflicts. The mod order window gives the mods the user picks a number inside the range below; the list of mods
does not set it.

| Key                            | Meaning                                                                                                                                  |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `format`                       | `mods_settings`: a section per mod folder with `Enabled` and `Priority`. `ue4ss_mods_txt`: a `Folder : 1` line per mod, in order (UE4SS). |
| `file`                         | `mods_settings`: a template in the folder the user chooses; the user can change the folder. `ue4ss_mods_txt`: a path relative to the deployment root, inside a slot's folder. |
| `priority_min`, `priority_max` | The numbers the user may give a mod, `1` to `9999` by default. The lower one loads first.                                               |

A `ue4ss_mods_txt` list is written into the profile's mod layer, not the game folder. A loader variant may name it
(see [loaders/README.md](../loaders/README.md)), and a game's own `load_order` wins over the variant's.

### `[[steps]]`

Programs run inside the overlay before the game starts (`phase = "before_start"`) or after it exits (`after_end`), when
a mod asks for them in `mod_steps.toml`: a script compiler, for example. `id`, `phase`, `program` (a path in the
layers, so a mod's own copy is used), `args` (`{game}` is the game folder), `when_dir`, `clean`, `outputs`, `log`.

### `[[logs]]`

Log files the game itself writes, or a log the overlay owns. The console shows their last lines after a launch, also
while the game runs. A mod's own `.log` files need no definition: every `.log` file in the mod layer and the overwrite
layer is listed automatically, so a new mod, a newer RED4ext or a different loader needs no definition change. The
interface translates the universal `kind` (`script`, `loader`, `console`, `crash`, `mod` or `game`); a definition never
carries user-visible text.

| Key         | Meaning                                                                                                                     |
| ----------- | --------------------------------------------------------------------------------------------------------------------------- |
| `kind`      | `script`, `loader`, `console`, `crash`, `mod` or `game`; the frontend translates it.                                        |
| `path`      | A file or a folder, relative to the game folder. `.` is the game folder itself.                                             |
| `pattern`   | The file name pattern used when `path` is a folder; `*` is the only wildcard, matching ignores ASCII case. Default `*.log`. |
| `limit`     | How many of the newest matching files a folder contributes. Default 4, from 1 to 20.                                        |
| `recursive` | Whether a folder is searched below its subfolders too. Default true; set false for the game folder itself.                  |

A file is read from the profile's overwrite layer first, then the mod layer, then the game folder. A missing file is
simply left out, so a definition may name a log before the first launch writes it.

### `[[tools]]`

Programs that come with the game and make files for it (`id`, `name`, `detect`, `output`). Gobbo runs them in the
overlay, so they see the mods, and what they write lands in the overwrite layer.

| Key             | Meaning                                                                                                                       |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `source`        | The tool's mod on a provider (`provider`, `mod_id`). Its archive is unpacked into the folder of `detect[0]`.                  |
| `prerequisites` | Installers the program needs in the Proton prefix (`name`, `url`, `sha512`, `args`, `check`, `check_starts_with`). See below. |

`[[tools.prerequisites]]`: Gobbo downloads the installer from `url` (https only), checks its SHA-512 and runs it through
Proton with `args`, for example `["/install", "/quiet", "/norestart"]` for a .NET runtime. It is installed when the folder
`check` in the prefix (relative to its `pfx` folder) holds a folder that starts with `check_starts_with`. The tool does not
start while one is missing, and the settings offer to install it after a confirmation. Windows needs none of this.

### `[install]`

The loader package a game can install: its name, version, the folder its release stands for (`to`), check paths,
`needed_by` folders and an HTTPS source with a SHA-256. The loader preset normally carries the shared package; a game's
`[install]` adds check paths or names its own build. Installing imports the release into the library as one mod at game
paths, so the overlay serves it like every other mod. When an enabled mod needs the package, the mod list offers a
one-click install. The package is offered, never forced: any enabled mod that ships the check files counts, so a build
from a mod site works too, and the setup check lets the user pick a copy. `to` may be empty for a loader whose root is
the deployment root. See the loader package section in
[loaders/README.md](../loaders/README.md) for the fields and the safety rules.

### Path templates

`{game}`, `{prefix}` (the Proton prefix), `{appdata}`, `{local_appdata}`, `{local_low}` (Windows `LocalLow`, inside the
prefix under Proton), `{documents}`, `{home}`, and in `profile_data` also `{settings}` (the folder of the order file).
For a Steam game with a Proton prefix the user folders are the ones Windows programs see inside the prefix. For an
install that is not from Steam, the prefix is the
one Heroic or Lutris names, the one the user picks, or else the one Gobbo owns in the game's data folder; it exists once
the game has been started. Under Proton the user folders always resolve inside the prefix, never to the Linux home
folders, so a template waits for the first start instead of moving. A game's `user_data` sets are the exception: their
path is known before the prefix exists, so a shared set is shown as unavailable until the first start instead of
blocking the mods. A native install (its folder has the
`linux_markers` or the `path_linux` program) resolves them in the Linux home folders. `..` is refused.

## Patterns

| Kind of game                          | Definition                                                                                                                                                                                                                             |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A mods folder, one folder per mod     | A slot: `dir = "Mods"`, with a `marker` when archives wrap the mod in other folders.                                                                                                                                                   |
| The whole game root is the layout     | `full_root = true`: every archive path is a game path; a bare file needs a `move` rule (Cyberpunk 2077).                                                                                                                               |
| Unity with BepInEx                    | `loader = "bepinex"` (or `variant = "6"` for IL2CPP). The preset holds the plugin slot, the core/config/patchers/monomod mirrors, the proxy root files and the archive aliases.                                                        |
| A game on a known loader              | `loader = "<id>"`; the preset in `loaders/<id>.toml` carries the shared `requires` and `[deploy]` rules. The game adds only what is specific to it.                                                                                    |
| Unreal Engine `.pak` mods             | `loader = "unreal"` and `root = "{game}/<Project>"`: the preset carries the flat `Content/Paks/~mods` slot, the `LogicMods` mirror and their aliases. Add `variant = "ue4ss-4"` (or `"ue4ss-3"`) for UE4SS Lua/C++ mods.               |
| A mods tree inside the game (`Data/`) | `root = "{game}/Data"` and a mirror per folder (`meshes`, `textures` ...); overlay still works. Plugins at the archive root and a plugin order file are not covered yet.                                                               |
| One global override tree              | A flat slot (`Data/Override`, `flat = true`) with `root_extensions` and `root_folder_extensions`, plus mirrors for path-sensitive folders; `resource_names = true` makes equal file names conflict across all of them (The Witcher 1). |
| Mods in the user's data folder        | `root = "{appdata}/<Game>"` (and `root_linux`); overlay covers that root and the game folder stays clean.                                                                                                                              |
| Loaders next to the executable        | `full_root = true` takes a new loader file or folder with no entry; a mirror is only for replacing a file the game ships.                                                                                                              |
| Mods packed with a root loader folder | `[deploy.aliases]` maps the folder to its game folder, for example `plugins` to `BepInEx/plugins` (LethalLib in Lethal Company). The game folder still needs its slot or mirror.                                                       |

## Supported games

| Game                              | Pattern                                                                                                                                                                                                                   | Tested |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| The Witcher: Enhanced Edition     | flat slot `Data/Override`; mirrors `Data/Music`, `Data/Movies`, `Data/Cutscenes`, `Data/Dialogues`, `Data/voices`, `Data/Scripts`, `Data/Templates`, `Data/Meshes`; `resource_names`; user data `{documents}/The Witcher` | no     |
| The Witcher 3: Wild Hunt          | slot `Mods` (marker `content`), mirrors `DLC`, `bin/config`, `bin/x64_dx12`; `mods.settings`; user data `{documents}/The Witcher 3`                                                                                       | yes    |
| Cyberpunk 2077                    | `full_root`; loose `.archive`/`.xl` moved to `archive/pc/mod`; the redscript step; crash report logs, with every mod `.log` discovered automatically                                                                      | yes    |
| TCG Card Shop Simulator           | `loader = "bepinex"`; Nexus and Thunderstore                                                                                                                                                                              | yes    |
| Lethal Company                    | `loader = "bepinex"`; Nexus and Thunderstore                                                                                                                                                                              | yes    |
| Tainted Grail: The Fall of Avalon | `loader = { id = "bepinex", variant = "6" }`; Nexus                                                                                                                                                                       | yes    |
| Valheim                           | `loader = "bepinex"`; Nexus and Thunderstore; native Linux build found and started directly                                                                                                                             | no     |
| R.E.P.O.                          | `loader = "bepinex"`; Nexus and Thunderstore                                                                                                                                                                              | no     |
| Content Warning                   | `loader = "bepinex"`; BepInEx mods only; Nexus and Thunderstore                                                                                                                                                           | no     |
| Palworld                          | `loader = { id = "unreal", variant = "ue4ss-4" }`; `root = "{game}/Pal"`; pak/IoStore, LogicMods and UE4SS Lua/C++ mods; Nexus                                                                                            | no     |
| Hell is Us                        | `loader = { id = "unreal", variant = "ue4ss-4" }`; `root = "{game}/HellIsUs"`; pak/IoStore, LogicMods and UE4SS Lua/C++ mods; signature bypass for signed paks; Nexus                                                     | no     |

The Witcher: Enhanced Edition covers loose files only. Custom adventures (`.adv` in `{documents}/The Witcher`),
campaign modules (`.mod`/`.mmd` in `Data/modules`), language packs (`.tlk`/`.key`) and installer-based overhauls
(Rise of the White Wolf, Full Combat Rebalance) are not managed yet. Its mirrors in `Data/Music`, `Data/Movies`,
`Data/Scripts`, `Data/Templates`, `Data/Meshes`, `Data/Dialogues` and `Data/voices` keep their paths; `resource_names`
makes equal names conflict across those mirrors and the flat `Data/Override` slot.

Valheim also ships a native Linux build. Its definition names that program and its marker, so the Games page finds the
native install and starts it directly, without Proton. The Windows program stays for installs that run under Proton.

Tainted Grail has two builds. The default Steam build is IL2CPP and takes BepInEx 6; the `mono` beta branch is Mono and
takes BepInEx 5. Both loaders use the layout described by the `bepinex` preset. The definition takes variant `6`: it
requires only `winhttp.dll`, not a core file, and also mirrors `dotnet/`. The Mono build's BepInEx 5 core is still
mirrored. Gobbo does not check which build is installed.

BepInEx and the pak tools are not installed by Gobbo yet. The Script Merger for The Witcher 3 comes from its Nexus mod and
its .NET runtime is installed into the Proton prefix.

The Witcher 3 user-data redirect is covered by the synthetic overlay test; check it on a real installation before
treating that part as verified.

Under Proton, Gobbo loads a DLL native when the launched game has it next to the program. The DLLs checked are the common
mod loaders (`winhttp.dll`, `winmm.dll`, `version.dll`, `dinput8.dll`, `dwmapi.dll` and others, in `src-tauri/src/games/proxy.rs`) and each DLL the
definition lists in `root_files` or mirrors (`bin/x64/version.dll`). Gobbo adds `n,b` for each one that is there, after
the names the launch settings already give; a loader no mod provides is not named. A game lists its own proxy in
`root_files` or as a mirror; the common loader names need no entry.

The Proton options (WineD3D, the Proton log, esync and fsync off, full-screen FSR, the FSR 4 and DLSS upgrades, NVAPI,
forced DXR, Mesa Anti-Lag, the low latency layer with its Reflex companion, DXVK low-latency frame pacing, the FSR 4 and
DLSS indicators, MangoHUD and a local shader cache), plus a frame rate cap for DXVK and vkd3d-proton, are in the game
settings on Linux, and Windows does not show them. The launch
settings stay for what a game itself needs, such as `DXVK_HUD=fps`. A Unity game may crash in the Vulkan loader on some
Proton builds; the WineD3D switch can avoid that, but it replaces DXVK with a much slower OpenGL path and can cut FPS
several times over. The latency switches depend on the Proton build and driver; turning on Mesa Anti-Lag clears the low
latency layer and the other way round, also in a hand-edited settings file. These are the user's choices, not defaults.

## User definitions

A definition can also live outside the repository, in the data folder:

- `<data>/definitions/games/<id>/game.toml`, with an optional `icon.webp` or `icon.png`;
- `<data>/definitions/loaders/<id>.toml` for a loader preset.

A user loader with the same id as a bundled or downloaded preset wins, so a user can test a new loader version
without rebuilding Gobbo. A user game with the id of a bundled or downloaded game is refused: the Games page names
the file and keeps the bundled or downloaded one. The same page reports an invalid file with its path; the rest of
the catalog still loads. The folder is read whenever
the Games page opens, so a new or edited definition needs no restart. **Games → Add your own game** writes this folder
for the user; a definition created there can be edited again from the Games page or from the game settings. The editor
shows the same loader presets as a hand-written file and can dry-run a draft against a mod archive with the import
rules; the archive check writes nothing.

A definition may carry the evidence that it was tested on a real installation:

```toml
[verified]
date = "2026-11-12"
os = ["windows", "linux"]
game_version = "1.2.3"
loader_version = "5.4.23"
```

The interface shows **Verified** with that evidence, and **Unverified** for a user definition without it. A bundled
definition without the table shows no badge. The table describes a real test; do not add it to a game nobody ran.

**Game settings → Advanced → Export definition** (or **Export definition** in the editor) writes `game.toml`, the icon
and any user loader the game names into one zip. Another Gobbo imports it with **Games → Import definition**, or by
dropping the zip on the window. The import preview shows the game, its loader, the files and any definition it would
replace before anything is written.

## The definition catalog

`catalog.json` at the repository root is the distribution file the app fetches: every shipped game and loader with
its SHA-256, ready for a Gobbo to download without an app release. It is generated, committed and guarded like
`src/bindings.ts`: `bun run catalog` rebuilds it from the folders, and `bun run guard` fails when a change under
`games/` or `loaders/` ships without regenerating it, so a definition change carries its catalog entry in the same PR.

The app reads the catalog once per launch, the first time the Games page opens, and compares each entry's checksum
with the local file. An install downloads only the picked ids — `game.toml`, the icon, and the loader preset a game
names when that loader is missing locally or out of date — verifies every checksum, validates the merged definition
in a throwaway folder and only then moves the files into `<data>/catalog/`, the loader before the game. A checksum
mismatch fails that id with a retryable error: GitHub may serve a newer file than its cached catalog for a few
minutes.

A definition that needs a newer Gobbo says so with `min_gobbo = "0.3.0"`, the release that first serves it correctly.
Set it when the file depends on a fix or a feature an older app lacks, also when an older app would still parse it: a
fix in the code does not change the file, so nothing else can tell. A loader preset takes the same key, and a game
inherits the higher floor of itself and its loader. An app older than the floor refuses the file with the version it
needs, and the catalog lists the entry as needing that Gobbo version without offering it. The game the user already
has keeps its current definition until they update. The `schema` key stays the gate for a change in the file format.

A downloaded definition is read-only like a bundled one and shadows the bundled file with the same id. After an app
update, the first start removes the downloaded copies that the new version ships itself, so a stale download never
hides a newer bundled file; the next check offers the repository version again if it differs. A downloaded
definition with no bundled counterpart stays installed, also when the repository drops it.
