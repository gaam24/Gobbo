# Gobbo Design System

This file is the design contract for the Gobbo frontend. Read it before changing anything under `src/`.
`AGENTS.md` describes the architecture; this file describes the visual system and the component rules.

## Identity

- Gobbo is a precise, calm desktop tool. It should feel like a workshop, not a landing page.
- The goblin mark and the green accent are the identity. Green is a signal, not decoration.
- The app is dark-only by design. It runs next to games, often in low light; a light theme would double the
  surface, status-colour and contrast work without a user request. Revisit only if a light theme is explicitly
  requested.
- One focal point per screen. If everything is emphasised, nothing is.

## Typography

- UI: **Schibsted Grotesk Variable**. It is the product voice and is deliberately not Inter or Geist.
- Data: **IBM Plex Mono** for paths, IDs, versions, sizes, counts and code. Not for labels, buttons, prose or
  keyboard keys.
- Dates and times use the UI font without `tabular-nums`: tabular figures in Schibsted Grotesk widen `:` and `,`, so
  "03:08" reads as "03 : 08". Keep `tabular-nums` for numbers that line up in a column.
- Scale and roles:

| Role                        | Class       | Size / line-height | Weight    |
| --------------------------- | ----------- | ------------------ | --------- |
| Page title                  | `text-xl`   | 20 / 28            | 600       |
| Detail title                | `text-lg`   | 18 / 28            | 600       |
| Section, dialog, card title | `text-base` | 16 / 24            | 600       |
| Body, control, label        | `text-sm`   | 14 / 20            | 400 / 500 |
| Meta, badge, timestamp      | `text-xs`   | 12 / 16            | 400 / 500 |

- Weights: **400, 500, 600**. No `font-bold`, `font-[550]` or `font-[650]`.
- Sentence case for UI text. Uppercase only for short labels, with `tracking-wider` (5%) or `tracking-widest`
  (10%). Never uppercase body text.
- Long prose: `max-w-[65ch]`, `leading-relaxed`, `text-pretty`. Aim for 45–90 characters per line.
- No text below 12 px. `text-[11px]` is a bug.

## Colour

Use the semantic tokens in `src/index.css`. Never use raw colours in components.

- Core: `background`, `foreground`, `card`, `popover`, `primary`, `secondary`, `muted`, `accent`,
  `destructive`, `border`, `input`, `ring`.
- Secondary text: `muted-foreground`. Tertiary text: `subtle-foreground`.
- Status: `draft`, `applied`, `win`, `lose`, `update`, `idle`. Status colour is always paired with text and,
  where useful, an icon or dot. Colour is never the only signal.
- Contrast: 4.5:1 for normal text, 3:1 for large text, UI boundaries and focus indicators.
- Tokens are written in `oklch()`. Surfaces are the warm grays of Claude Dark (hue 106, C 0–0.012); the colour
  comes from the goblin mark: primary is the face green `#23c385`. Destructive is a light warm red that keeps
  4.5:1 on the lighter surfaces and on its own 10/20% tints. Status colours are muted pastels with the same
  perceived lightness (L 0.68–0.78) and similar chroma (C 0.02–0.15); they stay distinguishable without
  competing with the primary.
- `border` is for decorative dividers. `input` is for form-control boundaries and must stay at 3:1 against
  the adjacent surface.
- `ring` is the focus colour; shadcn uses `ring-ring/50`, so the token itself is lighter than the text.

## Space

- Base unit: 4 px. Rhythm: 8 px. Use the Tailwind spacing scale; do not invent 18/22/28/36.
- Roles:

| Relationship          | Space   | Tailwind            |
| --------------------- | ------- | ------------------- |
| Icon to text          | 4–6     | `gap-1` / `gap-1.5` |
| Related inline items  | 8       | `gap-2`             |
| Label to control      | 6–8     | `gap-1.5` / `gap-2` |
| List row padding      | 12      | `px-3 py-2.5`       |
| Card / dialog section | 16      | `gap-4` / `p-4`     |
| Panel padding         | 20      | `p-5`               |
| Page gutter           | 24      | `px-6`              |
| Section gap           | 32      | `gap-8`             |
| Page top / bottom     | 24 / 32 | `pt-6 pb-8`         |
| Large breathing room  | 40–48   | `gap-10` / `py-12`  |

- Layout sizes: rail 72 px, header 48 px, toolbar 56 px, status bar 36 px, table header 36–40 px,
  mod row 56 px, game row 60 px, inspector 320–480 px (min 280, max 900).
- Page gutters: main 24 px, panel 20 px, dialog 16 px. Do not mix 24 and 32 on the same kind of screen.

## Shape and elevation

- Radius scale: `sm` 6, `md` 8, `lg` 10, `xl` 14, `2xl` 18, `3xl` 22, `4xl` 26.
- Controls, inputs, menus and inline cards: `rounded-lg`.
- Dialogs and sheets: `rounded-xl`.
- Badges and status pills: `rounded-full` / `rounded-4xl`.
- Keyboard keys: `rounded-sm`.
- Prefer a border or ring to a shadow. Shadows are for layers that actually float:
  menus/popovers `shadow-md`, dialogs/sheets/toasts `shadow-lg`. No `shadow-2xl`.
- One overlay for all modal layers: `bg-scrim/70 backdrop-blur-xs`, with a short fade/zoom and reduced-motion
  fallback.

## Motion

- Micro-interactions: 100–150 ms, `ease-out`.
- Menus and popovers: 100 ms.
- Dialogs: 150 ms. Sheets: 200 ms. Progress: 150–200 ms.
- No bounce or elastic easing. No decorative pulsing dots, blinking cursors or entrance animations.
- Respect `prefers-reduced-motion`.

## Components

- `src/components/ui/` is the generated shadcn layer. Do not hand-edit it. Add components with the shadcn CLI.
  The only allowed extensions are documented in `src/components/ui/README.md`.
- `src/components/app/` holds repeatable app patterns built from shadcn primitives: `StatusBadge`, `HelpHint` and the
  page layout in `page-layout.tsx`. Page and dialog shells live in `src/components/` (`PageFrame`, `ModalDialog`);
  settings rows and path actions live with their feature in `src/views/settings/`.
- Every screen is laid out like the mod list, with the parts in `page-layout.tsx`:
  - `PageHeader`: the 48 px header. The way back, the title, the explanation behind `HelpHint`, a short meta text and
    the screen's actions at the right. No `text-xl` page title and no paragraph under it.
  - `PageToolbar`: search and filters under the header. `PageBody`: the scrolling body; lists fill it edge to edge, a
    form or prose page asks for the gutters with `padded`.
  - `ListRow` (an `Item`, full width, bottom border, 56 px) for every list row, and `ListGroupHeader` for a group
    heading, drawn like a mod list separator with its count. No outlined card per row.
  - `PageEmpty`: the centred empty state of a screen or list.
  - `SettingsLayout`: the section list and a `max-w-3xl` content column; both settings screens use it.
  - `PageFrame` composes the header, the toolbar and the body for a screen that needs nothing else.
- Inside a dialog or the details panel, use the parts in `content-blocks.tsx`:
  - `SectionHeading` for a block heading (with its count and actions).
  - `BoxList` + `BoxListRow` for every list of records: one bordered box with dividers, capped with `scroll` when it
    can be long. No outlined card per item and no bare `grid gap-1` list.
  - `Disclosure` for anything that opens and closes. Never the browser's `<details>`.
  - `OutcomeRow` inside a `BoxList` for a dialog that reports what will happen (an apply preview, a check, a delete):
    an icon, what happens and one sentence on what it means. It is the closing dialog's pattern: the icon colour
    carries the outcome (`primary` good, `warning` worth a look, `destructive` lost) and the text says the same.
  - `ModalDialog` spaces its body 16 px apart and keeps `DialogFooter` (secondary button first, primary last) outside
    the scroll; a dialog does not add its own spacing wrapper.
- A custom component is allowed only when it encodes a domain concept or a repeatable rule and cannot be
  expressed by composing shadcn. Examples: `ModTable` (drag, resize, dynamic columns), `GameRail`,
  `Fomod` option cards, `GameIcon`, `GobboMark`, `PanelHandle`.
- Never create a second button, input, select, checkbox, tab, dialog, menu, toast or progress bar.
  The rules in this section are the component matrix; a new component must fit one of them.
- Every form uses `Field`; every list row uses `Item`; every empty state uses `Empty`; every tab uses `Tabs`;
  every filter uses `ToggleGroup`; every progress bar uses `Progress`; every toast uses Sonner.

## Settings and help

- A settings page uses a vertical section nav when it has more than four sections; five to seven is the comfortable
  range. Put the most-used section first and the destructive section last.
- Pick one save pattern per section: switches and single selects save immediately; typed fields and groups of fields
  use one explicit Save. Do not mix both inside one section.
- Keep the visible row to a label, the control and, at most, one short hint. Explanations, history and edge cases
  belong in `HelpHint` (the question mark next to the label), not in a permanent paragraph.
- A visible hint is for syntax, a constraint or a consequence the user must know before acting. A tooltip is for
  "why" and "when", not for the only instruction needed to complete the task.
- Actions on the current object belong next to that object. Profile actions live next to the profile switcher; they
  are not repeated in the inspector or in the game settings.
- A count such as "Showing x of y" is only useful while a filter is active. Hide it when everything is shown.
- Filter buttons always show their counts. The counts are how a user finds conflicts, missing requirements and updates
  without opening each filter, so they are never hidden behind a click.
- A group header (a page of parts, a separator) shows only what its rows share or add up to. An empty cell in a header
  stays blank; the dash `—` marks an empty value in a row, not in a header.
- Put advanced and destructive actions behind a menu or in the last section, never next to the everyday controls.
- One number, one place. A count the header or the filter bar already shows is not repeated in the inspector.
- A problem carries its fix. When the fix is one known action ("Turn on X", "Install the loader"), its button sits
  right next to the message instead of in a separate dialog.
- A list that needs the user puts those rows first. Rows that only confirm that something is fine fold away once
  something else needs a look.
- An idle state says nothing. A cell, a line or a button that only reports the normal case ("Up to date",
  "Update all (0)") is left out; it appears when it has something to say.

## Research

The rules above follow these sources:

- [Progressive disclosure (NN/g)](https://www.nngroup.com/articles/progressive-disclosure/): show the frequent
  choices first and make the step to the rest obvious.
- [Tooltip guidelines (NN/g)](https://www.nngroup.com/articles/tooltip-guidelines/): tooltips are brief, contextual
  and never the only place a task-critical instruction lives.
- [Why so many info tips are bad (NN/g)](https://www.nngroup.com/articles/info-tips-bad/): a question mark next to
  the thing it explains is read as local help.
- [Guidelines for app settings (Microsoft Learn)](https://learn.microsoft.com/en-us/windows/apps/design/app-settings/guidelines-for-app-settings):
  group related settings, keep the count low, prefer switches for binary choices and reflect changes immediately.
- [Settings page patterns (shadcn ecosystem)](https://adminlte.io/blog/shadcn-ui-settings-templates/): vertical
  section nav for five or more categories, a form column, and focused drawers/dialogs for sensitive flows.

## Voice

- Specific verbs: Deploy, Launch, Import, Rescan, Reinstall, Open folder.
- No generic CTAs, no fake metrics, no emoji, no invented testimonials.
- One label per concept. Do not repeat the same hint in a heading, a description and a tooltip.
- Errors say what happened and what the user can do. They do not blame the user.

## Decision log

- **Dark-only:** deliberate; the app sits next to games and mod tools. Documented here, not an accident.
- **OKLCH palette:** the tokens are written in OKLCH. Surfaces are the warm grays of Claude Dark (hue 106); the
  colour comes from the goblin mark (face green primary, light warm red destructive). `scripts/check-contrast.ts`
  parses `oklch()` and checks 59 pairs across the background, card, popover, accent, secondary and the
  input/muted surfaces.
- **Schibsted Grotesk + IBM Plex Mono:** brand character plus data legibility. Not the AI-default
  Inter/Geist pairing.
- **`@tanstack/react-virtual` for every long list:** the mod table, the card grid and the conflict explorer all use it.
  A hand-rolled spacer window changes its height while scrolling, which makes the browser adjust `scrollTop` and the
  list visibly jumps. The virtualizer positions items with transforms and measures them, so do not write another
  window hook — share `useScrollMargin` when a list sits below a sticky header or other content.
- **Custom `ModTable`:** a semantic `Table` cannot express drag reordering, column resizing, dynamic columns
  and separators without losing behaviour. Keep the grid; use shadcn primitives inside it.
- **Custom `GameRail`:** a 72 px icon rail with game tiles and context menus. `Sidebar` is too heavy and
  changes the layout model.
- **Custom Fomod option cards:** a group can require an option, offer "at most one" that the user can clear again, and
  mark options as conditional. Radix `RadioGroup` cannot express all three without changing what the installer means,
  so the card keeps the radio/checkbox role and uses the app's tokens.
- **Sonner:** the shadcn-supported toast. The old custom store is removed.
- **Command:** shadcn `Command` (cmdk) for the palette, with Gobbo's ranking function kept.
- **No light theme:** see above. Revisit only on request.
