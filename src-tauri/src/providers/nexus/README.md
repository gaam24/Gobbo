# The Nexus Mods client

The player-facing guide is [Nexus Mods and Thunderstore](../../../../docs/user/providers.md).

The module talks to three Nexus APIs, and only where each one is needed:

| API | Used for |
|---|---|
| v3 (`https://api.nexusmods.com/v3`) | mod pages, file versions, file lists, signed download links, file dependencies |
| v2 (`https://api.nexusmods.com/v2`) | the signed archive link of a collection revision |
| GraphQL v2 (`https://api.nexusmods.com/v2/graphql`) | a mod's summary, its creation/update dates, the requirements it lists for itself, and collection revisions; batched by aliases |
| v1 (`https://api.nexusmods.com/v1`) | `GET /users/validate.json` only; v3 has no user endpoint |

The v3 endpoints are marked **Experimental** by Nexus. Wire types are therefore tolerant: missing optional fields
deserialize to `None`, and a batch that fails leaves the previously saved values in place.

## Id glossary

Nexus uses several ids that look alike. Mixing them is the easiest way to get a 404.

| Name | Example | Used by |
|---|---|---|
| game-scoped mod id | `107` | `nxm://.../mods/107/...`, `/games/{domain}/mods/{id}` |
| global mod uid | `14315125997675` | `ModPage.uid`, `/mods/{id}/files` |
| game-scoped file version id | `168615` | `nxm://.../files/168615`, `/games/{domain}/mod-file-versions/{id}`, `download-link` |
| global file version id | `14315126166183` | `/mod-file-versions/{id}`, `/mod-file-versions/{id}/dependencies` |
| global mod file id | a string | `/mod-files/{id}/versions` |

## Download flow

1. `link.rs` parses the `nxm://` link with the `url` crate, so a percent-encoded key and expiry arrive intact.
2. The shared queue in `sources/downloads/queue/dedup.rs` deduplicates by `(game, mod id, file id)`.
3. `api/client.rs` asks v3 for signed CDN links. The request is **never retried after an HTTP answer**: the endpoint
   records a download on Nexus.
4. `download/file.rs` tries every CDN location into a unique `.part` file, rejects an HTML page, takes the name from
   `Content-Disposition`/URL/type in `download/target.rs`, and replaces the target atomically.
5. `download/sidecar.rs` writes the `.source.json` that names the provider, mod and file for the import.

An expired website key (HTTP 410), an invalid one (400), a quarantined file (403) and a rejected API key (401) are
separate errors, so the window can tell the user what to do.

## Collections

`collections/` implements the shared `CollectionSource` port. It reads a collection revision from GraphQL
(`collection` plus `collectionRevision`, one query for both), keeps the latest revision and the non-discarded revision
list, and stores the opaque `/v2/collections/<id>/revisions/<id>/download_link` path. `api/client.rs::get_v2` resolves
that path with the saved key; the answer is a signed CDN link, which the shared archive downloader fetches. The v2 base
is the host root, not `/v2`, because the path already starts with it.

The revision query is public: without a saved key the client sends no `apikey` header and the preview still reads the
metadata. The archive link and a download need a key. Collection links are `https://www.nexusmods.com/games/<game>/collections/<slug>`
(also the short form), a revision page, a `?revision=` query, or `nxm://<game>/collections/<slug>/revisions/<n>`.
`file_page_url` builds `?tab=files&file_id=<id>&nmm=1` for the guided free-account flow.

## Updating the page data

The update engine in `update/` fetches the v3 mod page, then `api/graphql.rs` asks for one chunk of summaries, dates
and own requirements, then v3 is asked only for the file requirements that are stale. The requests are built from the
generic `SourceUpdateRequest`; the feature in `sources/update` stores the generic `SourceUpdate` and merges the
user-typed requirements, alternatives and DLCs.
