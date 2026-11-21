# Linux and Steam

Gobbo runs natively on Linux. Games can be started from Gobbo, or from Steam with a launch option. Overlay uses
overlayfs in a user namespace and is the only deployment method.

## Start from Steam

Gobbo shows the exact launch option for each game. It has this form, with the path to the Gobbo executable:

```text
"/path/to/gobbo" run <game id> -- %command%
```

For example, for The Witcher 3 the game id is `the-witcher-3-wild-hunt`:

```text
"/home/you/Gobbo/gobbo" run the-witcher-3-wild-hunt -- %command%
```

Paste it into Steam: **Game properties → General → Launch options**. Only a game started this way sees the mods. The
game id is in the game settings; Gobbo also offers a copy button for the exact line.

## Proton

A Steam game runs with the Proton build Steam assigns to it by default. The game settings list the Proton builds found
on the computer and let you pick one; the choice is saved per game. The list reads the same folders Steam does: the
`compatibilitytools.d` of the Steam install, Valve's Proton in every library, the system-wide
`/usr/share/steam/compatibilitytools.d` and `/usr/local/share/steam/compatibilitytools.d`, and
`STEAM_EXTRA_COMPAT_TOOLS_PATHS`. A build outside Steam's folders can be chosen by its folder, and the refresh button
finds a build installed while the page is open.

The Proton options in the game settings are switches: WineD3D, the Proton log, esync/fsync off, full-screen FSR, the
FSR 4 upgrade, NVIDIA NVAPI, the DLSS upgrade, forced DXR, Mesa Anti-Lag, the low latency layer (with its Reflex
companion), DXVK low-latency frame pacing, the FSR 4 and DLSS indicators, the MangoHUD overlay and a local per-game
shader cache, plus a frame rate cap from 30 to 240 FPS or unlimited. The switches stay for what the game itself needs
and are saved per game; a line in the game's launch settings overrides the same variable.

WineD3D replaces DXVK with OpenGL for Direct3D 9–11 and can cut FPS several times over. A Unity game may crash in the
Vulkan loader on some Proton builds and the switch can avoid that, but leave it off for normal play — it does not affect
DX12/vkd3d. Full-screen FSR needs a Proton build with Wine FSR and a game in true full screen; the FSR 4 and DLSS
upgrades need a build that provides them (such as GE-Proton) and download a DLL on first use. NVAPI is for NVIDIA cards;
forced DXR is for DX12 games where vkd3d-proton does not enable ray tracing on its own.

The latency switches depend on the Proton build and the driver, and some of them exclude each other. Mesa Anti-Lag needs
Mesa 25.3 or newer with the anti-lag layer (or a patched build such as CachyOS) and an AMD GPU. The low latency layer
needs Proton-CachyOS 11 or newer, or the layer installed for Vulkan; its Reflex companion works only with it. DXVK
low-latency frame pacing needs a Proton-CachyOS build with that DXVK fork and applies to Direct3D 9–11. Turning one of
Mesa Anti-Lag and the low latency layer on clears the other, and the saved file drops a pair that was edited by hand;
do not combine DXVK low-latency with the layer's Reflex mode either. Proton-CachyOS also disables Mesa Anti-Lag while
the FSR 4 upgrade is on. The frame rate cap needs a build whose DXVK and vkd3d-proton still read the frame rate
variables (such as Proton-CachyOS).

The FSR 4 and DLSS indicators need a build that provides them (GE-Proton or Proton-CachyOS) and a game actually using
the matching upscaler. MangoHUD needs to be installed on the computer; Gobbo only sets the variable and the layout comes
from MangoHUD's own config, so it covers Direct3D 9–12 as those games run on Vulkan. The local shader cache needs a build
that supports it and only saves stutter after the first run. These are the user's choices, not defaults.

A game needs a Proton prefix before tools can run in it. Start the game once through Steam, then try again.

## Overlay

Overlay uses overlayfs in an unprivileged user namespace. It needs a kernel with overlayfs and unprivileged user
namespaces enabled. If the system does not allow them, Gobbo reports the reason and cannot deploy mods; there is no
method that writes the game folder as a fallback.

The layer covers the deployment root, including a root outside the game folder such as `%APPDATA%`. A user-data folder
outside the deployment root is copied around a launch on Linux; the game folder itself stays untouched.

The overlay is per profile. Switching a profile changes the layer; the game folder on disk stays as it was. Writes the
game or a tool makes go to the profile's layer and can be captured as a mod of their own or cleared.

## Games from other stores

A Windows game from GOG, Epic or a plain folder starts through Proton just like a Steam game. Gobbo keeps its prefix in
the game's data folder, so the game's own saves and settings work. The Proton build is picked in the game settings; the
newest installed build is used by default.

A game from Heroic or Lutris uses the prefix that launcher already runs it in, so mods and saves land where you play.
The game's Proton tab shows that prefix and lets you pick another folder or go back to Gobbo's own. Gobbo can start the
game itself only in a Proton prefix (a folder named `pfx`); a prefix made by plain Wine still works for mods and saves,
and the game starts from its launcher.

Until a Windows game outside Steam has started once, its prefix does not exist, so a mod folder inside it (for example
in `AppData`) cannot be resolved yet. Start the game once, then deploy.

## Native Linux builds

Valheim ships a native Linux build, and its definition names both the Linux program and the files that identify that
install. The Games page finds a native-only install and starts it directly, without Proton. What is in the game folder
decides: a Windows copy of the same game, for example one Steam installed with Proton forced, still runs through Proton. Other games offer the same
support as soon as their definitions name the native build.

## If something does not work

- **The game was not detected.** Add it with **Locate**, or check that the Steam library is on this computer. A native
  Linux build may not be covered by the current launcher.
- **Overlay is not available.** Check that unprivileged user namespaces are enabled, then try again. Gobbo cannot
  deploy mods without overlay.
- **The game does not start.** Read the game console in Gobbo, check the launch option, and try another Proton build.
  The [Troubleshooting](troubleshooting.md) guide has the log and support bundle steps.

The technical matrix for both platforms is in [PLATFORMS.md](../../PLATFORMS.md).
