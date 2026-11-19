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

By default the window's **X** hides Gobbo in the tray instead of ending it. The setting
**Settings → Interface → Close to the system tray** turns that off; with it off, **X** ends the app as before. On Windows
a left click on the icon opens the window and a right click opens the menu. On Linux click events are not delivered to
the app, so use the menu's **Open Gobbo** item.

A task that needs an installer choice or a browser step stays in **Working** while the window is hidden. Gobbo sends one
notification when such a task appears or fails, and only while the window is hidden or minimized. Progress updates never
notify.
