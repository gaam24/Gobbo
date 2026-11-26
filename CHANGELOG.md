# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses semantic versions.

## v0.2.0

### Added

- **GOG, Epic, Heroic, Legendary and Lutris games are found where they are installed.** Flatpak installs of Heroic and Lutris and minigalaxy's folder count too, and Epic games that are still downloading are left out.
- **The Witcher 3, Cyberpunk 2077 and Tainted Grail are found from GOG too.** A GOG copy appears on the Games page like a Steam copy.
- **Windows games from GOG, Epic and plain folders start on Linux through Proton.** Heroic and Lutris games use the prefix you play in, and other games keep one of their own, so mods and saves stay in one place.
- **The Proton tab works for games outside Steam too.** A Windows game from GOG or Epic lets you pick its Proton build, the newest one by default, and Heroic or Lutris games can use another prefix.
- **A game with a native Linux build starts without Proton.** Valheim's Linux install is found and started directly, and a Windows copy still runs through Proton.
- **The Games page names each game's store and lets you pick its install.** A badge shows Steam, GOG, Epic, Heroic, Legendary, Lutris or a manual folder, and a game on both Steam and GOG offers both folders when you add it.
- **UE4SS and BepInEx install with one click when a mod needs them.** The mod list offers the official release as soon as an enabled mod needs one, and plain pak mods never ask for UE4SS.
- **Pick which loader build a game uses.** A BepInEx or UE4SS build from Nexus or Thunderstore counts like the official one, and the setup check lets you switch between your copies.
- **Unreal Engine games can load pak, IoStore and UE4SS mods.** Palworld is the first game with this support, and UE4SS sits in your mods like any other mod, so the game folder stays clean.
- **A load order window sets the order the game loads your mods in.** Only the mods you add get a number, and UE4SS mods follow the same order.
- **Hide Gobbo when a game starts.** A setting, off by default, moves the window to the tray once the game is running.
- **New games arrive without an app update.** The Games page offers new and updated definitions from the repository, checked at each start unless settings turn that off, and you choose what to download; an app update also removes downloaded copies of games the app now ships.
- **A definition that needs a newer Gobbo says so.** It shows the version it needs and cannot be picked.
- **Adding a game recognises its engine.** Gobbo spots Unity and Unreal Engine games and picks the matching BepInEx or UE4SS setup.

### Changed

- **Closing the window asks what to do.** The closing dialog offers quit, minimize to the taskbar and hide in the tray, and a choice saved in settings skips it.
- **Background notifications also report finished tasks.** While the window is hidden, each kind of task has its own switch for finished and failed results in settings.
- **Mod priority moved to the load order window.** The priority field in a mod's details is gone, and the window numbers the mods you put in the order.
- **The load order has one name everywhere.** The mod table column and the game editor now say Load order instead of Load priority or Mod order.
- **Locate names the files a game folder must hold.** The message for a folder that is not the game's lists them, and a native Linux folder shows the file that found it.
- **An archive Gobbo can't place says what to do.** The message names the folders a mod has to be in, so you know what to repack.
- **A clash between two mods names both mods.** The deployment message says which mod puts a file where the other puts files inside it, so you know which one to turn off.

### Fixed

- **Mods reach a game whose mod folder is inside the game folder.** The game now sees them, and a mod loader among them, at the paths it reads them from.
- **Locate and Add accept the folder that holds a game.** When the game sits one folder down, Gobbo uses that folder, and a game saved with the wrong folder is repaired on start.
- **A GOG game with installed DLC is found.** An installed DLC no longer hides its base game.
- **The same community profile folder can be imported twice.** The second import gets its own profile name instead of failing.
- **The download button no longer names one mod site.** It invites you to get mods online from any supported source.
- **A readme with a name Windows can't store no longer blocks its mod.** Gobbo skips that file like any other file it doesn't need, and the mod imports.
- **An archive with several mods named in other alphabets imports.** Mods without Latin letters used to share one folder name, so the import failed.
- **A game parameter in quotation marks stays one parameter.** A value with spaces, such as a path, reaches the game whole instead of being split at each space.
- **A game, settings or mods folder typed with a leading ~ is found.** It means your home folder, and Gobbo saves the full path.
- **The newest Proton is the one with the highest version.** When Steam names no build for a game, Proton 10 is no longer passed over for Proton 9.
- **A game with a program path past 260 characters starts when no mods are applied.** Windows refused such a path, so the game did not start.
- **Adding a game offers the program that is the game.** A large data folder no longer hides the programs near the top of the game folder, and a checksum or crash tool ranks after the game.
- **The program field lists every program Gobbo found.** Typing a path no longer narrows the list to that one program, and Browse picks a file in the game folder.
- **Typing in the game editor keeps the cursor in its field.** Rows for programs, folders, aliases, tools and providers no longer lose focus after each letter.
- **The game id follows the name as you type it.** It used to keep the first letter you typed.
- **A folder pasted with its quotation marks is accepted.** Gobbo removes the quotes that Windows' Copy as path adds.
- **A slow folder scan cannot show the programs of an earlier folder.** The list always belongs to the folder on screen.
- **The programs of a game folder stay listed while you edit the path.** A space typed and removed used to empty the list.
- **A folder name with two dots is accepted.** `Team..Saves` no longer counts as a path that leaves the game.
- **Extensions typed in capitals are accepted.** They are saved in lower case, the way the game's files name them.
- **A move target with a final slash or backslashes is accepted.** Gobbo writes it with `/` and no final slash.
- **Every problem of a game definition is listed.** The check used to show only the first eight.

## v0.2.1 (Unreleased)

### Added

- **Hell is Us mods work in Gobbo.** Pak, IoStore and UE4SS mods install and deploy.
- **A game that needs administrator rights offers to restart Gobbo with them.** Windows asks for permission, and after the restart the game starts with mods; Gobbo says when to restart it normally again.

### Changed

- **Profile comparison names a mod by its family name.** A mod with several versions now shows the same name in the comparison as in the list history.
- **The closing dialog shows which choice keeps Gobbo running and which ends it.** Hiding to the tray has a green icon and Exit has a red one.
- **Game tools have their own menu.** File conflicts, load order, snapshots and the checks moved out of the Play button's menu into Tools.
- **Import is a button in the mod toolbar.** Update all appears only when there is something to update.
- **A warning about a turned-off requirement has a Turn on button.** One click enables the mod another mod needs.
- **The apply preview says what happens to the game.** Each row names an outcome, such as mods going in or files several mods share.
- **The mod list stays quiet about versions that need nothing.** "Up to date" and "No source" no longer repeat under every row.
- **The mod details show each fact once.** Facts from the mod page have their own heading, and names that repeat the title are left out.
- **The game's state shows on its rail tile.** A dot marks a running game or one with changes waiting.
- **Notifications are grouped by day and show what kind they are.** Import notices name the mods instead of the archive files.
- **The setup check puts what needs a look first.** Checks that pass fold away.
- **The Games page shows only the actions you use.** Search again and Add your own game stay in view; importing a definition and checking the catalog moved to the menu.
- **Adding your own game is split into four parts.** Basics holds what most games need, More options folds the rarer rules under one line each, Linux gathers every Linux-only field, and the definition file has its own page.
- **The game editor lists what's still missing as steps to do.** Empty fields no longer show up as red errors before you start.
- **Gobbo settings, game settings and the game editor share one layout.** The same header, the same section list and a centred column on wide windows.
- **The mouse's back and forward buttons move between screens.** Alt+← and Alt+→ do the same, and Esc leaves a settings or side page.
- **More keyboard shortcuts.** Ctrl+1–9 opens a game from the rail, Ctrl+, opens settings, Ctrl+O imports archives, Ctrl+Enter reviews the changes to apply, Delete removes the selected mod after asking, / jumps to the search, and Ctrl+S saves in the game editor.
- **Buttons show their shortcut in the tooltip.** The shortcut list has a new Moving around section.
- **Storage shows each game's share of the space.** The folder path moved to the folder button.
- **Settings say what each switch does instead of repeating its name.** The notification grid uses checkboxes.
- **The interface uses the same words for shared files everywhere.** A mod overrides another; "wins" and "loses" are gone.

### Fixed

- **The health check shows readable text for mods that need a turned-off mod.** It showed a raw text code before.
- **The applied notice no longer shows stray asterisks.** It also no longer points to a Play button while the game runs.
- **The details panel no longer says "Applied" while changes are waiting.**
- **Discarding changes has its own icon.** It no longer shares the Play icon.
- **The file conflicts window fits its list.** A short list no longer sits in a tall empty frame, and the count shows only while a filter is on.
- **Dates and times read normally.** They no longer show spaced-out punctuation such as "03 : 08".
- **A manual Proton install keeps its saves in one place before the first launch.** Gobbo no longer treats a save folder as two separate paths to copy.
- **A partial game settings file no longer resets every folder.** Gobbo keeps the folders the file still has and fills the missing ones with empty defaults.
- **Too many profile redirects stop the launch before the game starts.** Gobbo reports the problem instead of starting the game without the redirects.
- **An oversized update archive says why it was refused.** Gobbo reports the size or entry limit instead of calling the archive damaged.
- **Making a mod from the overwrite folder reports a folder it cannot resolve.** Gobbo no longer reads the wrong place when it cannot work out where mods go.
- **An unrecognised saved store is shown as a manual install.** Gobbo no longer labels the game with a Steam app id it cannot confirm.
- **Gobbo waits briefly if its data folders are busy at startup.** A short lock from another window no longer stops the app from opening.
- **A deleted mod file stays deleted after the next launch.** The overlay no longer loses a deletion when two changes reach the same folder together.
- **A delete Windows refuses no longer hides the mod file.** The game gets the real error and the file stays visible.
- **A game that fails to start with mods reports it right away.** Gobbo no longer waits the full start timeout for a child process that has already exited.
- **A 32-bit game no longer stays stuck when its start helper hangs.** Gobbo stops it after 20 seconds and reports the failure.
- **A game with hundreds of folders keeps the modded view.** The overlay no longer stops merging after 128 folders.
- **A search pattern that names several folders lists their mod files.** Patterns such as `data*\*.txt` return the matching files instead of "not found".
- **A 32-bit game starts faster.** The launcher reads the ntdll export table once instead of thousands of cross-process reads.
- **A large game no longer loads its whole program into memory to start.** Gobbo reads the headers and import data, with a fallback for unusual images.
- **A game whose mod folder uses characters outside the system code page starts.** Gobbo falls back to the short 8.3 path for the import table instead of refusing the launch.