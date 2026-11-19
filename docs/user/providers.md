# Nexus Mods and Thunderstore

Gobbo can take a download from Nexus Mods and Thunderstore, import it and put it in the right game. The two services
work differently: Nexus needs a saved API key for update checks and direct downloads, but an `nxm://` link carries its
own short-lived key and needs none; Thunderstore needs no key.

## Nexus Mods

### API key

An API key lets Gobbo read a mod's page, check for updates and download a file. Create one on the
[Nexus API page](https://www.nexusmods.com/users/myaccount?tab=api) and save it in
**Settings → Mod providers → Nexus Mods**.

The key is stored in the operating system's credential store: Credential Manager on Windows or Secret Service on
Linux. It is never written to `settings.json`, the log or the window. The backend uses it; the interface never sees it. If the keyring is locked,
Gobbo reports that the key could not be read or saved instead of pretending there is no key.

### Download with the Mod Manager button

Turn on `nxm://` links in **Settings → Mod Manager Download links**. The system registers Gobbo as a handler, and the
**Mod Manager Download** button on a Nexus mod page sends the file to Gobbo. The Downloads page shows the progress, speed and time left, and the
file installs itself when it is done. The option is off by default.

On Linux, the handler is registered by the app; a desktop that does not pass custom schemes to the app may need the
link opened once from the desktop environment.

### Direct downloads and Premium

A direct download from Gobbo needs a Nexus Premium account and a saved API key. Without Premium, use the **Mod Manager
Download** button on the mod page; the file arrives through `nxm://` and installs the same way. A file that Nexus will
not serve right now (quarantine, moderation or no permission) is reported with that reason.

### Update checks

Gobbo checks for a newer version only when you press the button. A check with a key compares the installed file with
the newest file on the mod page. **Ignore this version** hides a version until the provider lists another one; the
exact version and file id are remembered.

**Update all** shows what it will change before it starts: every mod with its current and newest version. A successful
run offers one **Undo** that puts every selected version back. A stopped run keeps the mods it already updated.

## Thunderstore

Thunderstore needs no API key. A game joins it with `[providers.thunderstore]` in its `game.toml`; the full schema is
in [games/README.md](../../games/README.md).

Turn on `ror2mm://` links in **Settings → Mod Manager Download links**. The **Install with Mod Manager** button on a
Thunderstore package page sends the package to Gobbo. The game is found from the package's community. The package's dependencies are listed as
requirements, and a downloaded archive named `owner-name-version.zip` is read for the mod and its version. The package
icon is saved with the mod, so the grid shows it.

Thunderstore update checks need no key. A check compares the package's version with the installed one and reports a
newer version.

## What Gobbo sends

Gobbo has no account and no telemetry. It contacts Nexus Mods or Thunderstore when you start a download, check for
updates, open a mod page or save a key. The API key is sent only to its own service, from the backend. The log and a
support bundle redact request keys and signed links.

The exact error is shown in the window and in the log. A rejected key, a locked keyring and a file the service will not
serve have separate messages; use [Troubleshooting](troubleshooting.md) when the answer is not clear.
