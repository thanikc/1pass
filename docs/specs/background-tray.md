# Background tray

Date: 2026-11-16. Today closing the window quits the app, so `op run` fails until it is started again. The app should stay reachable.

## Responsibilities (Tauri layer, `src-tauri/src/lib.rs`)

- Show a tray icon with a menu: **Open**, **Lock**, **Quit**. Left click opens the window.
- A close request on the main window hides the window. The process, the socket and the vault session keep running.
- **Quit** exits through the existing exit path (clipboard cleared, socket removed).
- An `op run` request or a second launch raises the hidden window, as they already raise a minimised one.

## Security behaviour

- Hiding does not lock. Auto-lock keeps counting idle time, and a hidden window produces no activity, so the vault locks after the configured idle time. With auto-lock set to "never" the vault stays unlocked in the tray until Lock or Quit.
- Approvals still require the window: a request raises it.

## Failure modes

- The tray icon cannot be created (no tray library): closing the window quits, as today. The window is never hidden without a tray icon having been created.
- The tray icon is created but the desktop shows no tray (stock GNOME): the window can be brought back by launching one-pass again. Documented in the README.

## Not included

A setting to choose between quit and hide, start minimised, autostart at login.
