# System tray

Gobbo keeps a small icon in the system tray. It is a status surface for the work that continues while the window is
hidden: imports, updates, collection installs and the game itself do not stop when you close the window.

## What the icon tells you

The icon and its tooltip follow the game Gobbo is showing:

- **Applied** — the enabled list matches what is in the game.
- **Changes waiting: N** — the list changed since the last **Apply**.
- **Applying… / Launching… / Stopping… / Updating…** — work is in progress.
- **Game running** — the game Gobbo started, or one it found running, is still open.
- **Needs attention: N** — a task is waiting for a choice or failed. The **Working** menu shows the count too.

The shape of the icon changes with the state, and the tooltip names the game, the profile and the state in words. On
Linux the desktop may draw the icon in one colour; the tooltip and the menu still carry the status.

## What you can do from the menu

- **Apply and play** (or **Play**, or **Stop game**) — the same next action the window shows in its header.
- **Undo apply** — available while a deployment exists and the game is stopped.
- **Profiles** — switch the active profile; the current one has a checkmark.
- **Games** — switch the game the tray follows, when more than one is added.
- **Working** — a task that is running, waiting for you or failed. Clicking one opens the window on the **Working** card.
- **Open Gobbo**, **Settings**, **Quit Gobbo**.

**Quit Gobbo** is the only way to end the app from the tray. If a game is running or work is in progress, Gobbo asks first
and can stop the game before it exits.

## Closing the window

By default the window's **X** asks what to do. You can quit, minimize to the taskbar or hide Gobbo in the tray, and save
the choice in **Settings → Interface → When closing the window**. On Windows a left click on the icon opens the window
and a right click opens the menu. On Linux click events are not delivered to the app, so use the menu's **Open Gobbo**
item. **Settings → Interface → Hide Gobbo in the tray when a game starts** does the same move once a game is running;
it is off by default.

## Notifications

While the window is hidden or minimized, Gobbo sends a system notification when a task fails or finishes, when a task
waits for your input, and when the window is hidden in the tray. The title names the game the task worked on. Progress
updates never notify, and neither does a task you cancelled. Clicking a notification restores the window and opens
**Working**.

**Settings → Interface → System notifications** turns them all off. Below it, a table has one row per kind of task
(import, mod update, applying mods, tool run, ...) with a switch for **Failed** and one for **Finished**, so you can keep
the failures of an import and silence its successes. A task waiting for your input has no switch of its own: the work
cannot go on until you answer, so only the main switch silences it.
