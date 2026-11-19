# Collections and modpacks

A collection is a recipe made by a curator: a list of mods, their exact files, installer choices, order rules and
sometimes bundled files. Gobbo installs the recipe into a profile. It does not store the collection as a mod in the
library; it remembers which collection revision a profile came from, so the collection panel can show what is missing
and install it again.

## Open a collection

Use **Install a collection** in the profile menu, the mods toolbar menu or the command palette. You can paste a Nexus
collection or a Thunderstore modpack. For Nexus:

- a collection page: `https://www.nexusmods.com/games/<game>/collections/<slug>`,
- the short page address: `https://www.nexusmods.com/<game>/collections/<slug>`,
- a revision page or a `?revision=<n>` query,
- an `nxm://<game>/collections/<slug>/revisions/<n>` link.

The **Add collection** button on Nexus works too: the link reaches the open window, or starts Gobbo and waits for the
window.

For Thunderstore, paste the modpack page: `https://thunderstore.io/c/<community>/p/<owner>/<name>/`, optionally with
`v/<version>/`, or the older `https://<community>.thunderstore.io/package/<owner>/<name>/`. A modpack is the list of
its dependencies, each at a pinned version; it has no archive, installer choices or order rules to read.

## What the preview shows

The preview reads the collection before anything changes:

- the name, author, description, curator instructions and size,
- every member with its source and whether it is already installed, in the library or missing; a search and the
  filters (to install, installed, optional, needs attention) narrow a long list,
- the optional members, which start unchecked,
- the bundled files and patches the collection carries,
- anything Gobbo cannot apply, such as a game-specific plugin order, an expected tool run or INI tweaks.

A new profile is the default target, and the active profile when the collection is already in it. **New profile**
creates one named after the collection; a profile made from a
mod pack cannot take a collection, because a pack update would replace its list. **Place in the list** puts the
collection at the top or the end of an existing profile.

## Automatic and guided download

**Automatic** asks the provider for every file and imports it. On Nexus it needs a Premium account and a saved API
key; Thunderstore needs no account, so a modpack always installs automatically.

**Guided** works with a free account. Gobbo opens the exact file page; press **Mod Manager Download** on Nexus and the
download is matched to the member and imported with the curator's installer choices. The download queue does not
install it a second time. Close the dialog or press **Skip for now** to leave the member for later; the collection
panel can resume it.

Bundled files, direct links and patches are handled without the browser in both modes. A direct file is downloaded only
from a public HTTPS address, and its size and MD5 are checked when the collection records them.

## Installer choices

A collection records the FOMOD options the curator picked. Gobbo applies them without opening the wizard. When a
collection has no stored choices for a FOMOD, Gobbo shows the usual installer wizard for that member; **Skip for now**
leaves it for later, and closing the wizard keeps it in the collection panel.

## Finish and apply

The install view shows the progress, the member that needs you now and the live state of every member. Each member
that arrives settles into the list, and the collection's block keeps the collection's order. **Continue** resumes a
paused install with the download mode the account allows. **Apply now** deploys the profile, like any other list.

The header badge shows the active profile's collections. Open it to see each member, skip or restore a member, open the
collection page, install what is missing, apply, or remove the collection from the profile. The badge marks a collection
with a pending update with an arrow next to the header icon and beside its revision in the menu. When a newer revision
exists, **Review update** opens its preview; installing it into the same profile keeps the existing members, adds what
is missing and switches off mods the new revision no longer lists. A mod you switched off by hand is left alone, and a
mod the curator brings back is switched on again. Removing a collection removes only the mods it added, unless another
collection in the same profile lists them; mods the profile had before, the library and the archives stay.

## Checking revisions

The revision saved when a collection was installed is not enough to know that its author published a newer one.
**Check revisions** in the collection badge menu, or in the collection panel, asks each installed collection's provider
for its current revision and saves the answer. The arrow on the badge and **Review update** in the panel follow that
answer, so the panel never has to be opened to discover a newer revision. The same check runs once automatically when a
game opens, for the active profile only; turn it off in Settings → Interface if you would rather check by hand. A
provider that cannot be reached keeps its last answer and does not stop the other collections from being checked; one
provider read per collection uses the same API budget as every other provider request.

## Several collections in one profile

A profile can hold several collections, for example two Nexus collections or a collection and a modpack. Turn on the
**Collection** column in the mod list (Columns menu, List group) to see which collection each mod belongs to. A mod two
collections share shows both names.

## Patches

Some collections ship binary patches for the files a curator changed. Gobbo applies a patch only when the installed
file has the CRC32 the collection recorded. A mismatch, a missing patch or a missing target file is reported in the
log and leaves the imported mod in place.

## What is not applied

The collection recipe can carry game-specific data Gobbo does not use yet:

- a plugin load order and plugin rules (for example `plugins.txt`),
- tools the curator expects the player to run,
- INI tweaks stored under `INI Tweaks/`,
- per-file overrides.

The preview marks these as unsupported or a warning. The mods are still installed and listed; the game-specific step
is left to the player. Updating a profile to a newer collection revision is not automatic: open the newer revision and
install it into the same profile, which keeps the existing members, adds what is missing and switches off the mods the
new revision removed. A mod stays in the library, so it can be enabled again from the mod list.

See [Nexus Mods and Thunderstore](providers.md) for the key and the download flow.
