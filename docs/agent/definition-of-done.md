# Definition of done

A change is done when all of these are true.

- [ ] The user-visible behavior is stated, and the code does what it says on both platforms.
- [ ] The change is data where the repository says data (a game, loader, path or setting), not a branch on a game id.
- [ ] The layer table holds; a lower layer does not call a higher one.
- [ ] No production `unwrap`, `expect`, `panic`, `todo` or `dbg`; every `#[allow]` has a `reason`.
- [ ] An error has a stable code and both locales; a secret never reaches the log or a notice.
- [ ] A test fails on the old code and passes on the new one; an OS-specific test is gated and named.
- [ ] A long task is checked in **Working** with two games: work started on game A still shows A's name after a dialog
      closes and the active game switches to B, and **Stop** and **Open** on that row act on A.
- [ ] A change to a command or an event runs `bun run check:rust`; the regenerated bindings and the command capability
      are part of the same change.
- [ ] The generated `src/bindings.ts` is up to date; no generated file was edited by hand.
- [ ] `bun run lint`, `bun test` and `bun run check:rust` pass; the changed UI was looked at in the real window.
- [ ] `CHANGELOG.md` has an entry for a user-visible change.
- [ ] The docs that own the behavior are updated; the README stays a landing page, and screenshots match the UI.
- [ ] The report names what was not tested and the residual risk.
