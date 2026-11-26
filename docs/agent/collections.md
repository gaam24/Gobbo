# Collections

A collection (a Nexus collection or a Thunderstore modpack) is a recipe, not a library mod. The profile stores a `CollectionOrigin` for each collection
installed into it (`src-tauri/src/domain/collection/`), and the recipe and its archive are cached under
`<game data>/collections/<provider>/<game>/<slug>/r<revision>/` (`collections/cache.rs`). The library and the deployment
never learn that a collection exists; deployment works on the profile list like any other list.

The feature module is `src-tauri/src/collections/`:

| Path          | Owns                                                                                            |
| ------------- | ----------------------------------------------------------------------------------------------- |
| `manifest/`   | reading `collection.json`, validating paths and counts, building `CollectionPlan` and its order |
| `inspect.rs`  | parsing the link, reading the API revision, fetching the archive, writing the cache             |
| `check.rs`    | refreshing the newest revision of the profile's collections through each provider's own adapter |
| `status.rs`   | per-member status against the library and one profile                                           |
| `session/`    | creating or updating the profile, waiting entries, skip/restore, finish, manual-done, remove    |
| `download/`   | the automatic loop, the guided item actions, importing one member, patches                      |
| `bundle.rs`   | turning `bundled/<fileExpression>` into an archive                                              |
| `external.rs` | direct HTTP downloads with size/MD5 checks                                                      |
| `patch.rs`    | bsdiff patches with a CRC32 guard                                                               |
| `cache.rs`    | cache paths and the plan file                                                                   |
| `model.rs`    | IPC shapes                                                                                      |
| `commands/`   | Tauri commands                                                                                  |

## Provider port

`providers::CollectionSource` is the provider-neutral port (`src-tauri/src/providers/port.rs`). A provider advertises it
with `SourceCapabilities.collections` and returns an adapter from `SourceProvider::collection()`. The Nexus adapter is
`providers/nexus/collections/`: it parses website and `nxm://` collection links, reads the GraphQL revision, downloads
the archive through the signed v2 link and builds the page URL. No other module knows the Nexus API.

The Thunderstore adapter is `providers/thunderstore/collections.rs`. A modpack is a package whose dependencies are the
members, so it sets `CollectionRevision.complete`: the plan is built from the API alone (`manifest::plan_from_api`, not
limited) and no archive is fetched. A Thunderstore version is packed into the ordered `revision` number
(`major * 1_000_000 + minor * 1_000 + patch`); `CollectionInfo.revision_label` keeps the text the user reads (`v1.2.3`,
`r49` on Nexus).

A provider member is `CollectionSource::Provider { provider, mod_id, file_id, part_id }`; the feature reads the provider
from the member and never names one. In the guided flow a member whose provider needs an account opens its file page; a
member whose provider needs none (`credential_account()` is `None`) downloads at once, like a bundle.

`SourceProvider::file_page_url` is the guided flow's second port: it returns the exact file page for a mod and file, or
`None` when the provider has no page. The feature opens that URL through the opener.

## Data flow

1. `inspect_collection` (or `start_collection`, which opens the same way) parses the link, reads the current revision
   from the provider and checks that the collection is for the game's provider key.
2. A cached plan of the same revision is reused. Otherwise Gobbo reads the cached archive, or downloads it with the
   saved key. It reads `collection.json`, validates every path, drops members it cannot map and
   records issues.
3. The plan keeps the API metadata for the member (name, picture, summary, size), but the manifest wins for phases,
   optional flags, choices, patches, bundles, rules and instructions.
4. `start_collection` writes the origin and puts each missing member into `profile.waiting` using the pack mechanism.
   A member already in the library joins the list at once; a member that is not becomes a waiting entry with its place.
   When the origin already exists, the update also reconciles `CollectionOrigin.disabled`: families the collection
   added but the new plan no longer names are switched off (`enabled = false`) and their waiting entries are dropped,
   unless another collection of the profile uses them. `reconcile_disabled` in `domain::collection` is the pure rule.
   A mod in `disabled` switches back on when a later revision lists it; a mod the user switched off by hand is not
   recorded and is never touched. A `limited` plan switches nothing off.
5. Automatic install downloads Nexus files through `SourceProvider::download`, imports through the normal import
   pipeline with the stored FOMOD choices, and applies patches after the import. Bundled and direct files follow the
   same import path. One failure does not stop the rest.
6. The guided flow asks the queue to expect a file (`Downloads::expect_file`) and opens the exact page. The matching
   `nxm://` download is marked with the owner and is **not** auto-installed; the window imports it through
   `import_collection_file` with the collection's choices. A ready download from an earlier run is adopted
   (`Downloads::adopt`).
7. `finish_collection` settles waiting entries that are now in the library, switches a `latest`/`prefer` member to the
   imported version when needed, records the families the collection added (`added`: only the ones it put into the
   list) and every member family of the list (`families`), and puts the added block in plan order. This is one profile
   edit and one history point.
8. `remove_collection` removes only the `added` families that no other collection in the profile lists in its `added`
   or `families`. A mod the list had before the collection stays. A family switched off by an update stays in `added`,
   so removing the collection also clears that leftover.

## Mod list column

`CollectionOrigin.families` feeds `ModView.collections` through `ProfilesView::collections_of`: the names of the active
profile's collections that list the row's family. The `collections` column of the mod list shows them.

## Revision checks

The newest revision is a snapshot in `CollectionOrigin.latest_revision`, written at install and at each check; the badge
(`CollectionBadge::from`) and the panel (`CollectionSessionView.update`) both read the one rule
`CollectionOrigin::has_update`. The panel must not read `CollectionPlan.info.latest_revision`: that is the snapshot of
the installed revision in the cache and never changes after install. `check_collection_updates` takes the profile id,
reads each origin's URL through that provider's `CollectionSource::parse_link` and `revision`, and saves only the
`latest_revision` of the origins whose answer changed. One provider failure is logged and skipped. The window runs the
command once per game when the setting is on, or from the badge menu and the panel at any time; the backend command
returns the fresh status in one step.

## Status and counts

`CollectionItemStatus` is derived, never stored. A member can be missing, queued in `profile.waiting`, in the library,
installed with the pinned file, satisfied by another version under a `latest`/`prefer` policy, in the library with a
different version, skipped, manual or unsupported. `CollectionCounts` is built from the same map. The panel and the
badge read these values; the window does not compute status from booleans.

## Plan format

`CollectionPlan.format` is `PLAN_FORMAT = 1`. A plan of another format is ignored and the collection is read again. The
plan is written atomically with the shared JSON writer. An archive is found by its archive extension in the revision
folder; the CDN name is never used as a path without the shared file-name cleaning.

## Security rules

- `collection.json` is untrusted. Paths are compared as `/`-separated relative paths and refuse `..`, empty parts,
  drive letters and absolute paths. The manifest is capped at 8 MiB and 10,000 members.
- Bundled trees are extracted only under the revision cache, with a size limit, then zipped into a fresh archive.
- Direct URLs must be HTTPS and must not name localhost, a private address, `.local` or `.internal`.
- A patch is applied only when the source file's CRC32 matches the manifest, and the source is capped in size.
- The collection archive link must start with `/v2/collections/`; the API key is never printed and the v2 client is
  only used for that link.

## Free accounts

The profile origin, the cache and the panel are the same for free and premium accounts. The difference is one path:
premium uses the provider download; free uses the exact file page plus the owned download expectation. The frontend
chooses the command (`install_collection` vs `prepare_collection_item`), so the backend request has no `mode` field.

## Tests

`collections::manifest` reads a real CET+Essentials archive (`src-tauri/tests/fixtures/collections/cete-essentials.7z`).
`providers::nexus::collections::tests::a_real_collection_reads_and_downloads` is `#[ignore]`d and needs
`GOBBO_NEXUS_KEY` and the network. The fixture tests run everywhere; the live test is run by hand before a release.
