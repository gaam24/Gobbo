---
name: gobbo-security-audit
description: Audit a change for path, archive, secret and IPC risks.
---

## Read

`src-tauri/AGENTS.md` error and IPC rules, the archive/path code, the capability file and the redaction tests.

## Do

Treat a mod archive and a provider response as untrusted. A path must stay under its expected root; reject `..`, absolute paths, symlinks that escape and case collisions. A zip/7z/rar entry has a size and count limit. A secret never enters the log, a support bundle or a UI notice. A command exposes only the paths it needs. A new dependency needs a license and maintenance check.

## Verify

Run the archive, path, redaction and permission tests. Try a tampered archive, a traversal name and a missing key. Run `bun audit --production` and `cargo deny check`. Report a residual risk; do not claim safety without a test.
