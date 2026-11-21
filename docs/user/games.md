# Supported games

A game is data in Gobbo, not code: one folder under `games/`, one `game.toml`, and an optional icon. The same rules
apply to every game, and a game definition never adds a branch to the program.

## Tested and ready for testing

| Game                              | Status                                  |
| --------------------------------- | --------------------------------------- |
| The Witcher 3: Wild Hunt          | Tested on a real installation           |
| Cyberpunk 2077                    | Tested on a real installation           |
| TCG Card Shop Simulator           | Tested on a real installation           |
| Lethal Company                    | Tested on a real installation           |
| Tainted Grail: The Fall of Avalon | Tested on a real installation           |
| Valheim                           | Not yet verified on a real installation |
| R.E.P.O.                          | Not yet verified on a real installation |
| Content Warning                   | Not yet verified on a real installation |
| The Witcher: Enhanced Edition     | Not yet verified on a real installation |

**Tested** means detection, import, apply, undo and launch were run on a real installation. A definition without that
test can still work, but treat it as unfinished: report what you find, and add the evidence to `game.toml` only after
the test.

The Witcher: Enhanced Edition currently covers loose files only. Custom adventures, campaign modules, language packs
and installer-based overhauls are not managed yet.

Some loaders and tools still need a manual install. BepInEx is not installed by Gobbo yet, including for TCG Card Shop
Simulator. The Script Merger for The Witcher 3 comes from its Nexus mod, with the .NET runtime it needs on Linux.

## Add a game

1. Open **Games** and choose **Add game**. Gobbo scans the Steam libraries on the computer; a game outside Steam can
   be added with **Locate**.
2. Choose the game folder. Gobbo reads the definition and shows the game, its profiles and its current state.
3. Import a mod archive, switch the mod on and press **Apply**.

A game that is missing from the list can be added as a user definition without rebuilding Gobbo:

- `<data>/definitions/games/<id>/game.toml`, with an optional `icon.webp` or `icon.png`;
- `<data>/definitions/loaders/<id>.toml` for a loader preset.

A user loader with the same id as a bundled or downloaded preset wins. A user game with the id of a bundled or
downloaded game is refused: the Games page names the file and keeps the bundled or downloaded one. The folder is read
whenever the Games page opens, so an edited definition needs no restart.

The full schema, the primitives and the test steps are in [games/README.md](../../games/README.md). A game ships only
after it has been tested on a real installation.

## New games without an app update

Gobbo does not wait for a release to support a new game. The first time the Games page opens after Gobbo starts, it
checks the Gobbo repository for the current list of official definitions — quietly, and only when there is a network.
The **Check for new game definitions** button next to **Rescan** asks again at any time.

- A banner above the games list names what is new: updated definitions and games you do not have yet.
- **View** opens the download dialog. Updates come pre-checked; pick the games you want, and Gobbo downloads only
  those. A loader a game needs comes with it.
- An entry that needs a newer Gobbo shows the version it needs and cannot be picked. The game you already have keeps
  its current definition until you update Gobbo.
- Downloaded definitions are read-only. After you update Gobbo, the downloads the new version already ships are
  removed, so an old download never hides a newer bundled definition.

The launch check can be turned off in the app settings, next to the collection update check.

## How mods reach the game

Gobbo has one deployment method: **overlay**. The game sees the mods through a layer, and the real game folder stays
untouched. Each profile has its own layer, so switching a profile changes what the game sees without moving files in
the game folder.

The layer covers the deployment root, also when it is outside the game folder (for example `%APPDATA%`). A game whose
mods live outside the game folder still keeps that folder clean. A game's saves and settings outside the folder are
handled by the user-data sets; on Linux they are copied around a launch, and on Windows the hook redirects them.

A deployment can add, replace or hide files without writing the game folder. **Undo apply** takes the deployment out
and leaves the layer ready for the next apply. A game whose platform cannot run overlay (no unprivileged user
namespaces on Linux, or no hook on Windows) cannot be deployed, and Gobbo says why.

## Linux

Valheim also ships a native Linux build. Its definition names the Linux program in `path_linux` and the files that
mark its folder, so the Games page finds that install and starts it directly, without Proton. The Windows program
stays for installs that run under Proton.

The overlay details, Steam launch options and Proton settings are in [Linux and Steam](linux.md).

## Request a game

Open an issue with the game name, the store page and whether the game has a native Linux build. If you can test the
definition on a real installation, say so; a tested definition is the fastest way to get a game shipped.
