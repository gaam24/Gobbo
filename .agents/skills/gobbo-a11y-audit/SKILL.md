---
name: gobbo-a11y-audit
description: Audit a screen for keyboard, screen-reader and contrast problems.
---

## Read

`DESIGN.md` accessibility rules, the screen's markup and `src/components/ui`.

## Do

Every control has an accessible name; every table uses the right roles and row index; the focused row stays visible. No positive tabindex, no keyboard trap, no color-only state. A progress or error message has `role=status`/`alert` when it appears. Check reduced motion and 200% zoom. Prefer a semantic element over ARIA.

## Verify

Keyboard-only pass (Tab, Shift+Tab, arrows, Enter, Escape), `bun run ui:contrast`, and an accessibility tree snapshot. Record what you could not check (AT-SPI, real screen reader) in the report.
