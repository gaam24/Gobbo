---
name: gobbo-perf-profile
description: Measure a performance claim before changing code.
---

## Read

The first-paint report (`bun run size:report` after `bun run build:web`), the relevant benchmark or test, and
`DESIGN.md` motion rules.

## Do

Capture a before number on the same machine and data: first paint, a 10k-row scenario, React render counts or an iai count. Change one thing. Do not optimize by removing a correctness check. Keep a lazy boundary; do not pull a screen into the first paint.

## Verify

Run the same measurement after. Report p50, p95, worst case and the number of samples; say when there is no baseline.
Keep the number in the change report; add a benchmark or test only when it can be stable. Run the full lint and test
gates.
