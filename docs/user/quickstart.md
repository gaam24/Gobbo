# Quick start

This is the shortest path from a fresh download to a mod running in a clean layer. For one game and one mod it takes
about five minutes. The [glossary](../ux/glossary.md) explains each label once.

## 1. Install Gobbo

Download the archive for your system from the [latest release](https://github.com/gaam24/Gobbo/releases/latest), check
it against `SHA256SUMS.txt`, unpack it and start Gobbo. Windows also has an NSIS installer. See
[Install and data](install.md) for the checksums and the system requirements.

The first start asks where Gobbo should keep its data. **Portable** keeps everything in the program folder; **Global**
uses the system's application data folder. You can change this later in the settings.

## 2. Add your game

1. Open **Games** and choose **Add game**. Gobbo scans the Steam libraries on the computer; a game from another store
   or a standalone install can be added with **Locate**.
2. Pick the game folder. Gobbo reads its definition and shows its profiles and current state.

If your game is not listed, it can be added as data without rebuilding Gobbo. See
[Supported games](games.md#add-a-game).

## 3. Import a mod

- Drop a `.zip`, `.7z` or `.rar` archive on the Gobbo window, or use **Your mods → Import**.
- If you have a Nexus `nxm://` or Thunderstore `ror2mm://` link, open it and Gobbo downloads the mod for you. A link
  needs no saved API key; a saved key is still needed for update checks and direct downloads. Thunderstore needs no
  key. See [Nexus Mods and Thunderstore](providers.md).

Some archives ask questions before they install. Those are **Installer options**; choose what the mod page asks for.
Gobbo keeps the choices so the mod can be reinstalled the same way.

## 4. Turn the mod on and check the list

Enable the mod in your **Profile**. The status bar now says **Changes waiting**: the list differs from what the game
last saw.

Before applying, look at two things:

- **Needs attention** — the mod requires another mod, a loader or a file the profile does not provide. Gobbo can offer
  to get it.
- The conflict view — when two mods provide the same file, one **Overrides** the other. You can pick the winning file
  explicitly.

## 5. Apply and play

Press **Apply and play**. Gobbo writes the enabled list into your profile's layer through the overlay and starts the
game. The real **Game folder** is never written to; **How mods reach the game** explains the layer in the interface.

On Linux, Gobbo also shows the exact Steam launch option for the game so it starts through the same layer. See
[Linux and Steam](linux.md).

## 6. If something goes wrong

- **Undo apply** removes the last apply and leaves the layer ready for the next one.
- **Play without mods** starts the game as it is in its folder, which helps confirm that a problem comes from the list.
- For a mod that crashes the game, use the guided search that narrows the list down to the one causing it.
- **History** has undo and redo for list changes, and **Snapshots** can restore a whole profile including its saves.

See [Troubleshooting](troubleshooting.md) for common errors and the support bundle.

## Where to go next

- [Glossary](../ux/glossary.md) — one label per concept, as used across the interface.
- [Mod packs](packs.md) — share an exact setup with someone else.
- [Collections and modpacks](collections.md) — install a ready-made list from Nexus or Thunderstore.
- [Supported games](games.md) — what ships today and how to add a game.
