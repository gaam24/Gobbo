# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses semantic versions.

## v0.1.1 (Unreleased)

### Added

- **Games from GOG, Epic, Heroic, Legendary and Lutris are found where they are installed.** Gobbo reads each launcher's own list, including the Flatpak versions of Heroic and Lutris and minigalaxy's folder.
- **A GOG or Epic game installed through Heroic or Lutris is recognised by its store.** It needs no extra setup in the game definition.
- **A GOG game with DLC is found again, and Epic skips unfinished downloads.** An installed DLC no longer hides its game, and a half-downloaded Epic game is not offered.
- **A Windows game from GOG, Epic or a plain folder now starts on Linux too.** Gobbo runs it through Proton with a prefix it creates itself, so the game's own saves and settings work.
- **The Proton tab works for non-Steam games too.** A Windows game from GOG or Epic lets you pick the Proton build it starts with, the newest one by default.
- **The Games page now shows where a game is installed.** A badge names the store: Steam, GOG, Epic, Heroic, Legendary, Lutris or a manual folder.
- **A game found in several stores lets you pick the install.** A game on both Steam and GOG, for example, offers both folders when you add it.
- **New games without an app update.** The Games page offers new and updated definitions from the repository, and
  you pick what to download. The check runs once per launch and can be turned off in settings.
- **Clear version requirements.** A definition that needs a newer Gobbo shows the version it needs and cannot be picked.
- **Downloads never hide newer built-in games.** After an app update, downloaded copies the app now ships are removed.
- **Closing Gobbo now asks what to do.** Pick quit, minimize to the taskbar or hide in the tray, and save the choice in settings.
- **System notifications for background work.** While the window is hidden, Gobbo reports finished and failed tasks;
  each kind of task can be turned on or off on its own in settings.
- **Hide Gobbo when a game starts.** A setting, off by default, moves the window to the tray once the game is running.
- **A game with a native Linux build starts it directly.** A Valheim install with the Linux binary is found and started without Proton, and a Windows copy still runs through Proton.
- **Heroic and Lutris games use the prefix you play in.** Mods and saves land in the prefix the launcher already uses, and the Proton tab lets you pick another one.
- **Mods of a Windows game outside Steam stay in one place.** Gobbo waits for the game's first start instead of using a folder that changes after it.
- **The Witcher 3, Cyberpunk 2077 and Tainted Grail are found from a GOG install too.** A GOG copy of these games appears on the Games page like a Steam copy.
- **Locate finds the game inside the folder you pick.** When the game sits one level down, Gobbo uses that folder, and a game added that way before is repaired once.
