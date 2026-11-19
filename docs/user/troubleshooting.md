# Troubleshooting

Most problems leave a clear message in the window and a line in `gobbo.log`. Start with the log, then make a support
bundle if you need to report the problem.

## First steps

- Find the data folder in **Settings → Files and folders → Storage type**. `gobbo.log` is in that folder;
  `gobbo.log.1` is the backup above 4 MB.
- Use **Settings → Support → Create support bundle**. The zip has the log, the version and, when a game is selected,
  its definition, setup check and last run. Paths and secrets are removed.
- Say which platform, Gobbo version and game you use. A screenshot of the message helps; the support bundle supplies
  the rest.

## Overlay is not available

- **Linux:** overlay needs overlayfs and unprivileged user namespaces. Check that the kernel allows them, then try
  again. Gobbo cannot deploy mods without overlay.
- **Windows:** the portable archive includes the hook DLL and the helper programs. A build from source may be missing
  them; rebuild the overlay hook, or put the DLL next to Gobbo.
- **The game keeps its mods outside the game folder:** the layer covers that root too, but the game must be started
  through Gobbo or the Steam launch option.
- **The path is too long for overlayfs:** move the game or the mod library closer to the root of the drive.
- The game console's **Mounts** tab lists every prepared deployment with the layer that receives writes, and warns when
  the last run wrote into the game folder. It also lists layers left behind by deleted profiles or removed user-data
  sets; those can be removed after selecting them and confirming. Cleanup is refused while the game runs.

## The game was not detected

Open **Games → Add game** and use **Locate** to point at the game folder. A game outside Steam can be added the same
way. A native Linux build may not be covered by the current launcher; see [Linux and Steam](linux.md).

## The game does not start

- Check the Steam launch option; the game must be started through Gobbo or through the option Gobbo shows.
- Try another Proton build in the game settings.
- A game that has never run through Steam has no Proton prefix. Start it once through Steam, then try the tools again.
- Read the game console in Gobbo. A crash prompt can save a dump; the log names the last step.

## Apply is blocked

Gobbo writes nothing when the plan has a problem. The health check names each one and offers the fix:

- **Needs attention:** a mod needs a loader, another mod or a file the profile does not provide. Enable the
  requirement or install the loader.
- **The game is running:** close it. Applying and undoing wait until it exits.
- **A foreign file:** the game folder already has a file Gobbo does not manage. It refuses to overwrite it. Remove or
  rename the file, or choose a method that does not write into the game folder.
- **Not enough space:** free space on the drive that holds the library or the game.
- **Game folders are not set:** set the game folder and, for a game that needs it, the settings folder.

## A mod changed after installation

Gobbo records a checksum of a mod's files when it imports the archive. Select the mod, open **Files** and choose
**Verify files**. Gobbo says whether the files match the recorded checksum, changed since the import, or had no
checksum yet. For a mod imported before checksums existed, the first check records the current files as the baseline.
If the files changed, **Compare with archive** lists what is missing, extra or has another size; reinstall the mod
from its archive to restore it.

## Nexus and keys

- **The key was rejected:** create a new key on the Nexus API page and save it again.
- **The keyring is locked:** unlock the system keyring and try again. Gobbo does not fall back to a plain file.
- **A direct download needs Premium:** use the **Mod Manager Download** button on the mod page; it arrives through
  `nxm://`. Thunderstore needs no key.
- **A download will not start:** the provider may have quarantined or moderated the file. The message says which
  reason applies; the mod page is the fallback.

## Data and updates

- **I cannot find the library:** open **Settings → Files and folders → Storage type**. Portable data is inside the
  program folder; global data is in the system's application data folder. A removed game waits on the Storage page and
  can be restored.
- **I want to move the setup:** in portable mode, move the program folder with its `data` folder. Profiles and packs
  can be exported separately. The game folder, Proton build and launch environment are asked for again.
- **An update failed:** the old version stays installed. Read the message, check the log, and try again; the update
  does not delete the library.

## Report a bug

Open an issue with the [bug report form](https://github.com/gaam24/Gobbo/issues/new/choose). Include the Gobbo
version, the platform, the game, the steps and the support bundle. Do not put API keys, personal paths or save files in
a public issue.

A security problem goes through [SECURITY.md](../../.github/SECURITY.md), not the public tracker.
