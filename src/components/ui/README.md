# The shadcn layer

This folder is the generated shadcn layer. Treat it as vendored code: add and update components with the CLI,
not by hand.

```bash
bunx shadcn@4.23.0 add <component>
bunx shadcn@4.23.0 add --overwrite <component>
bunx shadcn@4.23.0 add --dry-run <component>
bunx shadcn@4.23.0 add --diff <component>
```

`components.json` uses the `radix-nova` style, the unified `radix-ui` package and the Lucide icon library.
The registry may be newer than the installed CLI; review `--diff` before `--overwrite`.

## Rules

- No app imports: `@/app/*`, `@/lib/*`, `@/views/*`, `@/i18n`.
- No app colour tokens (`bg-surf`, `text-ink`, `border-line`, `bg-brand`, `text-danger`).
- No app state, stores or Tauri calls.
- Use the semantic tokens from `src/index.css` (`background`, `foreground`, `card`, `popover`, `primary`,
  `secondary`, `muted`, `accent`, `destructive`, `border`, `input`, `ring`).
- Keep the exported API of the registry component. Do not trim exports just because the app does not use them
  yet; the next screen may need them, and the CLI will restore them on the next update.

## Allowed extensions

These are the only intentional deviations from the registry. Keep them small and documented here.

1. `closeLabel?: string` on `DialogContent` and `SheetContent`. The app passes `t("common.close")`; the default
   stays `"Close"` so the generated component still works outside the app.
2. The shared overlay style `bg-scrim/70 backdrop-blur-xs` on `DialogOverlay`, `AlertDialogOverlay` and
   `SheetOverlay`. One overlay for every modal layer.
3. `sonner.tsx` is adapted to a single dark theme: no `next-themes`, `theme="dark"`, and the toast surface uses
   the app's popover tokens.
4. `ResizableHandle` stays canonical. The inspector/console grip, if needed, lives in
   `src/components/panel-handle.tsx`.
5. `DialogTitle`, `SheetTitle` and `AlertDialogTitle` use `font-semibold` with `leading-tight` to match the
   app's section-title role (the registry uses `font-medium`).

Anything else belongs in `src/components/app/` as a wrapper or pattern, not here.
