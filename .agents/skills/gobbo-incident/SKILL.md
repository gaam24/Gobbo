---

name: gobbo-incident
description: Handle a crash, data loss, stuck game or broken release.
---

## Read

`gobbo.log`, the support bundle, the last run report, `docs/state-machines.md`, and the changelog.
## Do

Preserve evidence first: copy the log and the data folder path, do not clear state. Reproduce with the smallest fixture. Find the one source of truth that disagreed. Fix the cause and add a regression test. If user data may be affected, stop writing and tell the user which backup to keep. Do not hide a failure behind a retry.
## Verify

The original reproduction no longer fails; the new test fails on the old code. Run the full gates. Add a changelog entry and, for a release issue, a rollback note in `docs/release.md`.
