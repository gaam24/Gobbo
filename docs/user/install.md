# Install and data

Gobbo ships as a portable archive on Windows and Linux. On Windows there is also an NSIS installer. The first start
asks where the library, profiles and settings should live.

## Download

Take the archive for your system from the [latest release](https://github.com/gaam24/Gobbo/releases/latest).

| Asset                                         | What it is                                                  |
| --------------------------------------------- | ----------------------------------------------------------- |
| `gobbo-<version>-windows-<arch>-portable.zip` | The program, ready to run. Unpack it anywhere.              |
| `gobbo-<version>-windows-<arch>.zip`          | The NSIS installer built by Tauri.                          |
| `gobbo-<version>-linux-<arch>-portable.zip`   | The program, ready to run. Unpack it anywhere.              |
| `SHA256SUMS.txt`                              | The SHA-256 of every archive in the release.                |
| `sbom-*.cdx.json`, `THIRD-PARTY-NOTICES-*.md` | The software bill of materials and the third-party notices. |

The portable archive contains the program and the `games/` and `loaders/` data folders. On Windows it also contains the
overlay hook and helper programs. It does not contain a `gobbo.storage.json`; the first start creates it after you
choose a storage mode.

### Check the archive

On Linux:

```bash
sha256sum -c SHA256SUMS.txt
```

On Windows PowerShell, compare the output with the line for your archive:

```powershell
Get-FileHash .\gobbo-<version>-windows-<arch>-portable.zip -Algorithm SHA256
```

## Requirements

### Windows

- 64-bit Windows with WebView2 and the Microsoft Visual C++ runtime used by the build.
- A working credential store for the Nexus API key.
- Overlay loads a hook DLL into the game process. ARM64 Windows is experimental: it must compile, but
  it has not been tested on real hardware.

### Linux

- 64-bit Linux with WebKitGTK and GTK.
- A running keyring (Secret Service). The Nexus API key is never written to `settings.json`.
- For overlay: a kernel with overlayfs and unprivileged user namespaces. Overlay is the only deployment method; the
  [Linux and Steam](linux.md) guide explains the limits.

The app is a Tauri 2 program, so the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) are a useful
reference for the system packages.

## First start

The first start asks where Gobbo should keep its data:

- **Portable** — `<program folder>/data`, with the choice recorded in `<program folder>/gobbo.storage.json`. The
  library, profiles, settings and logs stay with the program, so the folder can be moved as a whole.
- **Global** — `%APPDATA%\dev.gobbo.modsorganizer` on Windows, or `$XDG_DATA_HOME/dev.gobbo.modsorganizer`
  (`~/.local/share/dev.gobbo.modsorganizer`) on Linux.

An existing installation asks once after an update. The global folder stays the default and its data is not moved.
To ask again, delete the choice file; its path is shown in **Settings → Files and folders → Storage type**.

If `GOBBO_DATA_DIR` is set, Gobbo uses that folder as a custom location and skips the question.

Two things stay outside the data folder: the Nexus API key lives in the operating system's credential store, and the
`nxm://` and `ror2mm://` handlers are an opt-in system integration. Neither moves with a portable folder.

The interface is available in English and Polish; the language is chosen in the app settings.

## Where your data lives

The data folder holds the mod library, archives, profiles, settings, logs and saved snapshots. The exact layout is an
implementation detail; back up the whole folder rather than single files.

- **Back up:** copy the data folder while Gobbo is closed. For a portable install, copy the whole program folder.
- **Move to another computer:** in portable mode, move the program folder with its `data` folder. A global install
  needs the data folder copied to the same path on the new computer, or a portable install with the folder moved next
  to the program.
- **Delete:** deleting a portable program folder deletes its library. A global data folder is outside the program
  folder, so uninstalling the program leaves it in place; delete it yourself when you want the space back.
- **Machine-specific values:** the game folder, the Proton build, the mods folder and the launch environment are local
  settings. They are not part of an exported profile or pack; the new computer asks for them again.

## Logs and support

Gobbo writes `gobbo.log` in the data folder, with one backup (`gobbo.log.1`) above 4 MB. The log records the start, a
migration that moved something, a state file that had to be set aside, and a failure that happened before the window
could show a message. Request keys and signed links are redacted.

For a report, use **Settings → Support → Create support bundle**. The bundle is a zip with the log, the version and,
when a game is selected, its definition, setup check and last run; paths and secrets are removed. See
[Troubleshooting](troubleshooting.md) for the common errors and the issue tracker.
