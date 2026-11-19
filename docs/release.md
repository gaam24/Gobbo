# Release notes

The app version has one owner: `release.json` at the repository root. The copies in `package.json`,
`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `README.md`, this file and the issue template are asserted against
it by `bun scripts/check-release-meta.ts` (part of `bun run lint`), and the updater tests read it directly. Change
`release.json` and the copies together; the check names any copy that was left behind. `CHANGELOG.md` is the source of
the release notes.

## Update channel

The updater endpoint and its public key live in `src-tauri/tauri.conf.json` (`plugins.updater`). The private key is
**not** in this repository; the local development copy is at `~/.config/gobbo/updater.key`. Back it up: without it no
future update can be signed.

The public key belongs in the repository. A normal debug or local build does not need the private key, because
`createUpdaterArtifacts` is kept in `src-tauri/tauri.updater.conf.json` and is used only for a release.

## Sign a release

The release workflow signs every update archive and publishes the manifest. There are four: the installers (NSIS on
Windows, AppImage on Linux) and the portable archives of both systems. A portable copy has no installer, so the app
replaces its program files itself (`src-tauri/src/update/`). Two repository secrets are needed, set in
**Settings → Secrets and variables → Actions**:

- `TAURI_SIGNING_PRIVATE_KEY`: the contents of `~/.config/gobbo/updater.key`;
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: its password.

`bun scripts/build-release.ts --updater` refuses to run without the key, so an unsigned release cannot start. Each OS
job then signs its portable archive with `tauri signer sign`, and stages a fragment for every update archive:
`dist/updater/*.nsis.zip` on Windows and `*.AppImage.tar.gz` on Linux (copied under a name without spaces), and the
portable archives (already release assets, recorded by name). The publish job runs `scripts/updater-manifest.ts merge`,
which writes `latest.json` and refuses a manifest that lacks any of the four platforms. The release gets `latest.json`
and the updater archives next to the installers.

The manifest has this shape, and every URL points at an asset of the same release. The example uses the next version
(`0.1.1`); the metadata check matches only references to the current release, so this illustration may stay as it is.

```json
{
	"version": "0.1.1",
	"notes": "Gobbo 0.1.1",
	"pub_date": "2026-11-12T00:00:00.000Z",
	"platforms": {
		"windows-x86_64": {
			"signature": "<contents of the .sig file>",
			"url": "https://github.com/gaam24/Gobbo/releases/download/v0.1.1/Gobbo-Mods-Organizer_0.1.1_x64-setup.nsis.zip"
		},
		"linux-x86_64": {
			"signature": "<contents of the .sig file>",
			"url": "https://github.com/gaam24/Gobbo/releases/download/v0.1.1/Gobbo-Mods-Organizer_0.1.1_amd64.AppImage.tar.gz"
		},
		"windows-x86_64-portable": {
			"signature": "<contents of the .sig file>",
			"url": "https://github.com/gaam24/Gobbo/releases/download/v0.1.1/gobbo-0.1.1-windows-x64-portable.zip"
		},
		"linux-x86_64-portable": {
			"signature": "<contents of the .sig file>",
			"url": "https://github.com/gaam24/Gobbo/releases/download/v0.1.1/gobbo-0.1.1-linux-x64-portable.zip"
		}
	}
}
```

The endpoint in the configuration is `https://github.com/gaam24/Gobbo/releases/latest/download/latest.json`. A fork
changes the endpoint, the public key and the repository name in the workflow. A local signed build needs the same two
variables in the shell: `bun scripts/build-release.ts --updater`.

**Portable copies** download their archive, check its signature through the updater plugin (with the portable key), and
replace only the program files next to the executable. The `data` folder, the library and the settings stay. Gobbo
refuses the update while a game it started is running, then starts the new copy and exits the old one. An update
replaces `games/` and `loaders/` with the versions of the release; a file the release does not ship stays.

## Verify before publishing

1. `bun run lint`, `bun run test`, `bun run check:rust`, `bun run build:web`.
2. Run the release workflow with `dry_run` to build and sign both platforms without publishing.
3. Test the update path once before the first public release, in a private copy of the repository: release `v0.1.0`,
   then `v0.1.1`. Test both the installer and the portable archive: install or unpack `v0.1.0`, open **Settings →
   Updates**, install `v0.1.1` and check that the app restarts on the new version with its data unchanged.
4. Publish the release only after that test works. A tag on `main` publishes `latest.json` at once, and every installed
   copy checks it.

Never commit `updater.key`, a password or a signed manifest for an unpublished release.

## Workflow

`.github/workflows/release.yml` runs the CI checks first. A manual run with `dry_run` builds both operating systems,
writes checksums and uploads the archives with a build-provenance attestation, but does not create a release tag.

A real release needs one version in `release.json` and every copy asserted by `bun scripts/check-release-meta.ts`; the
workflow refuses a mismatch. The archives are attested with `actions/attest-build-provenance`; verify one with:

```bash
gh attestation verify dist-release/gobbo-0.1.0-linux-x64-portable.zip --repo <owner>/<repo>
```

Publishing still uses `SHA256SUMS.txt`. Each build also produces a CycloneDX SBOM and a third-party notices file with
`bun run sbom`; both are attached to the release. The updater private key is a release secret, never a file in the
repository.

## SBOM and notices

`bun run sbom` writes `target/sbom/sbom.cdx.json` (CycloneDX 1.5) and `target/sbom/THIRD-PARTY-NOTICES.md`. The
components come from `cargo metadata` for Rust and from the installed `node_modules` packages for JavaScript; the
release workflow attaches one pair per operating system to the release. Run it after `bun install`, because a missing
`node_modules` leaves the JavaScript part out.

## Code signing

The project has no platform code-signing certificate yet. Windows SmartScreen and the Linux archive therefore show an
unsigned build; the updater signature and the build-provenance attestation still let a user check the file. Choose a
provider (Trusted Signing, SignPath or a hardware token) before claiming a signed release. The updater key is unrelated
to OS signing and stays in `~/.config/gobbo/updater.key`.

## Roll back a release

1. Keep the previous version's archives and `latest.json`. The updater only moves forward, so a user on a bad build
   needs the previous archive.
2. Delete or replace the bad `latest.json` asset and mark the release as a draft:
   `gh release edit vX.Y.Z --draft` (or delete the asset with `gh release delete-asset`). The
   `releases/latest/download/latest.json` URL then falls back to the newest published release.
3. If the bad release is the newest one, publish the previous version's `latest.json` from its release (or re-upload
   it) so the update check offers a working version again.
4. Add a `CHANGELOG.md` entry that names the bad version and the version to install.
5. Never delete the old binaries: a user may already have the bad build and no network path back.
