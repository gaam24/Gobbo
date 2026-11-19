# Mod packs

A mod pack is a recipe, not a zip of someone else's files. It records the name, version and author, the mods with
their exact files and versions, the installer choices, the order, the separators and the rules. It also carries the
mod groups the author made (including translation groups and their language), and the author's user-data isolation
choices (shared or per profile, by set id), so a rebuilt profile keeps them; a pack from an older Gobbo leaves those
choices at the receiver's defaults. Group members travel as positions in the pack's mod list, so a group survives a
rebuild even when the receiving computer assigns different internal ids; the group ids themselves are local handles and
are made fresh on import. Archives travel inside only when the author chooses; otherwise the receiver gets the files
from Nexus Mods or Thunderstore.

## Export a pack

**Export mod pack** in the profile menu writes a `.gobbopack` file. Before the export, Gobbo lists what would stop a
rebuild: a missing archive, a file that is no longer on the provider, a choice the pack cannot record. Fix those first
or accept that the receiver will see the same warnings.

The pack is small when it does not carry archives. Share it only with the permission of the mods' authors; the pack
records their work, not a license to redistribute it.

## Import a pack

**Import mod pack** opens a preview before anything changes:

- what is already in your mods,
- what to get from Nexus Mods or Thunderstore,
- what changes in a profile made by an earlier version of the pack,
- the exact versions the pack expects.

A file that is already in the library is matched by its provider id, file id and version. A missing file is downloaded
for you when Nexus Premium and an API key allow it; otherwise the preview names the mod page and the **Mod Manager
Download** button is the fallback. A pack that carries its archives works without the internet.

## Pack links

A pack link names one `.gobbopack` file and the SHA-256 it must have. Version 1 has two forms:

```text
gobbo://pack?url=https%3A%2F%2Fexample.com%2Fpack.gobbopack&sha256=<64 hex characters>&name=pack.gobbopack
https://example.com/pack.gobbopack?sha256=<64 hex characters>
```

- `url` and the HTTPS address must use HTTPS. A plain `http` link is refused.
- `sha256` is required. The download is removed when the file does not match, so a tampered pack never reaches the
  preview.
- `name` is optional. Only the last path segment is kept, and the file name always ends in `.gobbopack`.
- A file larger than 32 GiB is refused. The download can be stopped from the window; a stopped download leaves no file.

Opening a `gobbo://` link hands it to the running window, or starts Gobbo and keeps the link until the window asks for
it. A pack preview needs a game, so a link that arrives before a game is chosen is reported in the notice list.

## When a pack does not rebuild

- **An archive is missing.** The preview names the mod and its page. Download the exact version from the provider, or
  ask the author for a pack that carries the archive.
- **The provider lists a newer version.** The pack keeps the version it recorded; use **Update to** only when you want
  to leave the recipe.
- **The pack was made by a newer Gobbo.** The preview says which pack format it needs. Update Gobbo rather than
  guessing.
- **A profile changed.** The preview lists the mods the pack adds, removes or switches before you confirm. The current
  profile is kept until you accept.

The pack format is unchanged by a link. After the hash check, Gobbo opens the normal pack preview and the normal import
writes the profiles. See [Nexus Mods and Thunderstore](providers.md) for keys and downloads.
